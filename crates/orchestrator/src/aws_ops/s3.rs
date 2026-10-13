use crate::errors::{error::RsOrchestratorResult, s3_presign::RsS3PresignError};
use aws_config::SdkConfig;
use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, SharedCredentialsProvider},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, KeyInit, Mac};
use ref_sync_shared::env_vars::EnvVars;
use serde_json::json;
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};
use time::OffsetDateTime;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const S3_BUCKET_USER_ARTIFACTS: &str = "S3_BUCKET_USER_ARTIFACTS";
const SERVICE: &str = "s3";

pub struct S3Config {
    pub user_artifacts_bucket: String,
}

impl S3Config {
    pub fn from_env() -> RsOrchestratorResult<Self> {
        Ok(Self {
            user_artifacts_bucket: EnvVars::env_var(S3_BUCKET_USER_ARTIFACTS)?,
        })
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
}

impl S3 {
    pub fn new(sdk_config: &SdkConfig) -> RsOrchestratorResult<Self> {
        let credentials_provider = sdk_config
            .credentials_provider()
            .ok_or(RsS3PresignError::MissingCredentials)?;
        let region = sdk_config
            .region()
            .map(|region| region.as_ref().to_string())
            .unwrap_or_else(|| "us-east-1".to_string());
        Ok(Self {
            config: S3Config::from_env()?,
            credentials_provider,
            endpoint_url: sdk_config.endpoint_url().map(str::to_string),
            region,
        })
    }

    pub async fn signed_post(
        &self,
        key_prefix: &str,
        expires_in: Duration,
    ) -> RsOrchestratorResult<PresignedPost> {
        let credentials = self
            .credentials_provider
            .provide_credentials()
            .await
            .map_err(RsS3PresignError::Credentials)?;
        self.presigned_post(&credentials, key_prefix, SystemTime::now(), expires_in)
    }

    fn presigned_post(
        &self,
        credentials: &Credentials,
        key_prefix: &str,
        now: SystemTime,
        expires_in: Duration,
    ) -> RsOrchestratorResult<PresignedPost> {
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
            now + expires_in,
            credentials.session_token(),
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
                "https://{}.s3.{}.amazonaws.com/",
                self.config.user_artifacts_bucket, self.region
            ),
        }
    }

    fn policy(
        bucket: &str,
        key_prefix: &str,
        credential: &str,
        amz_date: &str,
        expires_at: SystemTime,
        session_token: Option<&str>,
    ) -> serde_json::Value {
        let mut conditions = vec![
            json!({ "bucket": bucket }),
            json!(["starts-with", "$key", key_prefix]),
            json!({ "x-amz-algorithm": ALGORITHM }),
            json!({ "x-amz-credential": credential }),
            json!({ "x-amz-date": amz_date }),
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

#[cfg(test)]
mod tests {
    use super::*;

    const ACCESS_KEY_ID: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET_ACCESS_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const FIXED_TIME: Duration = Duration::from_secs(1440938160);

    fn credentials() -> Credentials {
        Credentials::new(ACCESS_KEY_ID, SECRET_ACCESS_KEY, None, None, "test")
    }

    fn s3() -> S3 {
        S3 {
            config: S3Config {
                user_artifacts_bucket: "examplebucket".to_string(),
            },
            credentials_provider: SharedCredentialsProvider::new(credentials()),
            endpoint_url: None,
            region: "us-east-1".to_string(),
        }
    }

    #[test]
    fn signing_key_matches_reference() {
        let signing_key = S3::signing_key(SECRET_ACCESS_KEY, "20150830", "us-east-1");
        assert_eq!(
            hex::encode(signing_key),
            "61c08448a068b7aaaa3bd62d8e7b3c83b7982fcb0cae7650b7334230c1e715b6"
        );
    }

    #[test]
    fn presigned_post_matches_reference() {
        let now = SystemTime::UNIX_EPOCH + FIXED_TIME;
        let post = s3()
            .presigned_post(&credentials(), "user/eric/", now, Duration::from_secs(3600))
            .unwrap();
        assert_eq!(
            post.url,
            "https://examplebucket.s3.us-east-1.amazonaws.com/"
        );
        assert_eq!(post.fields["key"], "user/eric/${filename}");
        assert_eq!(post.fields["x-amz-date"], "20150830T123600Z");
        assert_eq!(
            post.fields["x-amz-credential"],
            "AKIAIOSFODNN7EXAMPLE/20150830/us-east-1/s3/aws4_request"
        );
        assert_eq!(
            post.fields["policy"],
            "eyJjb25kaXRpb25zIjpbeyJidWNrZXQiOiJleGFtcGxlYnVja2V0In0sWyJzdGFydHMtd2l0aCIsIiRrZXkiLCJ1c2VyL2VyaWMvIl0seyJ4LWFtei1hbGdvcml0aG0iOiJBV1M0LUhNQUMtU0hBMjU2In0seyJ4LWFtei1jcmVkZW50aWFsIjoiQUtJQUlPU0ZPRE5ON0VYQU1QTEUvMjAxNTA4MzAvdXMtZWFzdC0xL3MzL2F3czRfcmVxdWVzdCJ9LHsieC1hbXotZGF0ZSI6IjIwMTUwODMwVDEyMzYwMFoifV0sImV4cGlyYXRpb24iOiIyMDE1LTA4LTMwVDEzOjM2OjAwLjAwMFoifQ=="
        );
        assert_eq!(
            post.fields["x-amz-signature"],
            "2775a3b5a1a0b9afaa436d391b7b65050f15ef8303ebe6014364a4b2f9f3069d"
        );
    }
}
