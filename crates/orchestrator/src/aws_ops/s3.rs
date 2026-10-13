use crate::errors::{
    error::{RsOrchestratorError, RsOrchestratorResult},
    s3_presign::RsS3PresignError,
};
use aws_config::SdkConfig;
use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, SharedCredentialsProvider},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, KeyInit, Mac};
use ref_sync_shared::{
    deadline::Deadline,
    env_vars::EnvVars,
    errors::{config::RsConfigError, error::RsSharedError, timeout::RsTimeoutError},
    timeouts::Timeouts,
};
use serde_json::json;
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};
use time::OffsetDateTime;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const DEFAULT_MAX_OBJECT_SIZE_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_SIGNED_URL_EXPIRY: Duration = Duration::from_secs(60 * 60);
const MAX_SIGNED_URL_EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_OBJECT_SIZE_BYTES_KEY: &str = "S3_MAX_OBJECT_SIZE_BYTES";
const MIN_OBJECT_SIZE_BYTES: u64 = 0;
const AWS_ENDPOINT_URL_S3: &str = "AWS_ENDPOINT_URL_S3";
const S3_BUCKET_USER_ARTIFACTS: &str = "S3_BUCKET_USER_ARTIFACTS";
const SIGNED_URL_EXPIRY_KEY: &str = "S3_SIGNED_URL_EXPIRY";
const SERVICE: &str = "s3";

pub struct S3Config {
    pub user_artifacts_bucket: String,
    pub max_object_size_bytes: u64,
    pub signed_url_expiry: Duration,
}

impl S3Config {
    pub fn from_env() -> RsOrchestratorResult<Self> {
        let signed_url_expiry =
            EnvVars::duration(SIGNED_URL_EXPIRY_KEY, DEFAULT_SIGNED_URL_EXPIRY)?;
        if signed_url_expiry > MAX_SIGNED_URL_EXPIRY {
            return Err(Self::invalid_expiry());
        }
        Ok(Self {
            user_artifacts_bucket: EnvVars::env_var(S3_BUCKET_USER_ARTIFACTS)?,
            max_object_size_bytes: EnvVars::u64(
                MAX_OBJECT_SIZE_BYTES_KEY,
                DEFAULT_MAX_OBJECT_SIZE_BYTES,
            )?,
            signed_url_expiry,
        })
    }

    fn invalid_expiry() -> RsOrchestratorError {
        RsSharedError::Config(RsConfigError::invalid(
            SIGNED_URL_EXPIRY_KEY,
            format!(
                "must be at most {} seconds (7 days)",
                MAX_SIGNED_URL_EXPIRY.as_secs()
            ),
        ))
        .into()
    }
}

pub struct PresignedPost {
    pub url: String,
    pub fields: BTreeMap<String, String>,
}

impl PresignedPost {
    pub fn fields_json(&self) -> RsOrchestratorResult<String> {
        Ok(serde_json::to_string(&self.fields)?)
    }
}

pub struct S3 {
    config: S3Config,
    credentials_provider: SharedCredentialsProvider,
    endpoint_url: Option<String>,
    region: String,
    timeouts: Timeouts,
}

impl S3 {
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsOrchestratorResult<Self> {
        let credentials_provider = sdk_config
            .credentials_provider()
            .ok_or(RsS3PresignError::MissingCredentials)?;
        let region = sdk_config
            .region()
            .map(|region| region.as_ref().to_string())
            .unwrap_or_else(|| "us-east-1".to_string());
        let endpoint_url = EnvVars::optional_env_var(AWS_ENDPOINT_URL_S3)?
            .or_else(|| sdk_config.endpoint_url().map(str::to_string));
        Ok(Self {
            config: S3Config::from_env()?,
            credentials_provider,
            endpoint_url,
            region,
            timeouts,
        })
    }

    pub async fn signed_post(
        &self,
        key_prefix: &str,
        deadline: Deadline,
    ) -> RsOrchestratorResult<PresignedPost> {
        let timeout = self.timeouts.operation_timeout(deadline)?;
        let credentials =
            tokio::time::timeout(timeout, self.credentials_provider.provide_credentials())
                .await
                .map_err(|_| {
                    RsSharedError::Timeout(RsTimeoutError::Operation {
                        operation: "S3 provide_credentials",
                    })
                })?
                .map_err(RsS3PresignError::Credentials)?;
        self.presigned_post(
            &credentials,
            key_prefix,
            SystemTime::now(),
            self.config.signed_url_expiry,
        )
    }
}

impl S3 {
    fn presigned_post(
        &self,
        credentials: &Credentials,
        key_prefix: &str,
        now: SystemTime,
        expires_in: Duration,
    ) -> RsOrchestratorResult<PresignedPost> {
        let expires_at = now
            .checked_add(expires_in)
            .ok_or_else(S3Config::invalid_expiry)?;
        let (date, amz_date) = Self::amz_date(now);
        let credential = format!(
            "{}/{}/{}/{}/aws4_request",
            credentials.access_key_id(),
            date,
            self.region,
            SERVICE
        );
        let policy = Self::policy(
            &self.config.user_artifacts_bucket,
            key_prefix,
            &credential,
            &amz_date,
            expires_at,
            credentials.session_token(),
            self.config.max_object_size_bytes,
        );
        let policy_base64 = BASE64.encode(serde_json::to_string(&policy)?);
        let signing_key = Self::signing_key(credentials.secret_access_key(), &date, &self.region);
        let signature = hex::encode(Self::hmac_sha256(&signing_key, policy_base64.as_bytes()));
        let mut fields = BTreeMap::from([
            ("key".to_string(), format!("{key_prefix}${{filename}}")),
            ("policy".to_string(), policy_base64),
            ("x-amz-algorithm".to_string(), ALGORITHM.to_string()),
            ("x-amz-credential".to_string(), credential),
            ("x-amz-date".to_string(), amz_date),
            ("x-amz-signature".to_string(), signature),
        ]);
        if let Some(token) = credentials.session_token() {
            fields.insert("x-amz-security-token".to_string(), token.to_string());
        }
        Ok(PresignedPost {
            url: self.bucket_url(),
            fields,
        })
    }

    fn bucket_url(&self) -> String {
        match &self.endpoint_url {
            Some(endpoint) => format!(
                "{}/{}",
                endpoint.trim_end_matches('/'),
                self.config.user_artifacts_bucket
            ),
            None => format!(
                "https://{}.s3.{}.{}/",
                self.config.user_artifacts_bucket,
                self.region,
                Self::dns_suffix(&self.region)
            ),
        }
    }

    fn dns_suffix(region: &str) -> &'static str {
        if region.starts_with("cn-") {
            "amazonaws.com.cn"
        } else {
            "amazonaws.com"
        }
    }

    fn policy(
        bucket: &str,
        key_prefix: &str,
        credential: &str,
        amz_date: &str,
        expires_at: SystemTime,
        session_token: Option<&str>,
        max_object_size_bytes: u64,
    ) -> serde_json::Value {
        let mut conditions = vec![
            json!({ "bucket": bucket }),
            json!(["starts-with", "$key", key_prefix]),
            json!({ "x-amz-algorithm": ALGORITHM }),
            json!({ "x-amz-credential": credential }),
            json!({ "x-amz-date": amz_date }),
            json!([
                "content-length-range",
                MIN_OBJECT_SIZE_BYTES,
                max_object_size_bytes
            ]),
        ];
        if let Some(token) = session_token {
            conditions.push(json!({ "x-amz-security-token": token }));
        }
        json!({
            "expiration": Self::expiration(expires_at),
            "conditions": conditions,
        })
    }

    fn signing_key(secret_access_key: &str, date: &str, region: &str) -> Vec<u8> {
        let secret = format!("AWS4{secret_access_key}");
        let date_key = Self::hmac_sha256(secret.as_bytes(), date.as_bytes());
        let region_key = Self::hmac_sha256(&date_key, region.as_bytes());
        let service_key = Self::hmac_sha256(&region_key, SERVICE.as_bytes());
        Self::hmac_sha256(&service_key, b"aws4_request")
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn amz_date(time: SystemTime) -> (String, String) {
        let date_time = OffsetDateTime::from(time);
        let date = format!(
            "{:04}{:02}{:02}",
            date_time.year(),
            u8::from(date_time.month()),
            date_time.day()
        );
        let amz_date = format!(
            "{date}T{:02}{:02}{:02}Z",
            date_time.hour(),
            date_time.minute(),
            date_time.second()
        );
        (date, amz_date)
    }

    fn expiration(time: SystemTime) -> String {
        let date_time = OffsetDateTime::from(time);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
            date_time.year(),
            u8::from(date_time.month()),
            date_time.day(),
            date_time.hour(),
            date_time.minute(),
            date_time.second()
        )
    }
}
