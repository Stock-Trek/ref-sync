use crate::{
    error::{RsWebhookError, RsWebhookResult},
    infisical::{
        aws_auth::{InfisicalAwsAuthRequest, InfisicalTokenResponse},
        secrets::InfisicalSecretsResponse,
    },
};
use aws_config::{Region, SdkConfig};
use aws_credential_types::{Credentials, provider::ProvideCredentials};
use aws_sigv4::http_request::{
    PayloadChecksumKind, SignableBody, SignableRequest, SigningInstructions, SigningParams,
    SigningSettings, sign,
};
use aws_smithy_runtime_api::client::identity::Identity;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    time::SystemTime,
};

const HEADER_CONTENT_LENGTH_KEY: &str = "content-length";
const HEADER_CONTENT_TYPE_VALUE: &str = "application/x-www-form-urlencoded";
const HEADER_SESSION_TOKEN_KEY: &str = "x-amz-security-token";
const INFISICAL_AWS_LOGIN_PATH: &str = "/api/v1/auth/aws-auth/login";
const INFISICAL_SECRETS_PATH: &str = "/api/v4/secrets";

pub struct InfisicalAwsAuth {
    http_client: reqwest::Client,
    base_url: String,
}

impl InfisicalAwsAuth {
    pub fn new(http_client: reqwest::Client, base_url: String) -> Self {
        Self {
            http_client,
            base_url,
        }
    }
    pub async fn try_load_secrets(
        &self,
        sdk_config: &SdkConfig,
        identity_id: &str,
        project_id: &str,
        environment: &str,
    ) -> RsWebhookResult<HashMap<String, String>> {
        let region = Self::region(sdk_config);
        let credentials = Self::credentials(sdk_config).await?;
        let endpoint = Self::endpoint(&region);
        let headers = Self::headers(&credentials);
        let body = Self::body();
        let identity = credentials.into();
        let instructions =
            Self::signing_instructions(region.as_ref(), &identity, &endpoint, &headers, &body)?;
        let signed_headers = Self::signed_headers(instructions, &endpoint, &headers, body.len())?;
        let token_response = self
            .aws_login_response(&self.base_url, &signed_headers, &body, identity_id)
            .await?;
        let secrets_response = self
            .secrets_response(
                &self.base_url,
                &token_response.access_token,
                project_id,
                environment,
            )
            .await?;
        let mut secrets = HashMap::new();
        for secret_response in secrets_response.secrets {
            secrets.insert(secret_response.secret_key, secret_response.secret_value);
        }
        Ok(secrets)
    }
}

// private helper functions
impl InfisicalAwsAuth {
    fn region(config: &SdkConfig) -> Region {
        config
            .region()
            .cloned()
            .unwrap_or_else(|| Region::from_static("us-east-1"))
    }

    async fn credentials(config: &SdkConfig) -> RsWebhookResult<Credentials> {
        config
            .credentials_provider()
            .ok_or_else(|| RsWebhookError::MissingCredentials)?
            .provide_credentials()
            .await
            .map_err(RsWebhookError::InvalidCredentials)
    }

    fn endpoint(region: &Region) -> String {
        format!("https://sts.{}.amazonaws.com", region)
    }

    fn headers(credentials: &Credentials) -> BTreeMap<String, String> {
        let mut headers = BTreeMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE.as_str().into(),
            HEADER_CONTENT_TYPE_VALUE.into(),
        );
        if let Some(token) = credentials.session_token() {
            headers.insert(HEADER_SESSION_TOKEN_KEY.into(), token.into());
        }
        headers
    }

    fn body() -> String {
        let mut params = BTreeMap::new();
        params.insert("Action".to_string(), vec!["GetCallerIdentity".to_string()]);
        params.insert("Version".to_string(), vec!["2011-06-15".to_string()]);
        params
            .iter()
            .flat_map(|(k, values)| values.iter().map(move |v| (k, v)))
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn signing_instructions<'a>(
        region: &'a str,
        identity: &'a Identity,
        endpoint: &'a str,
        headers: &'a BTreeMap<String, String>,
        body: &'a str,
    ) -> RsWebhookResult<SigningInstructions> {
        let signable_request = SignableRequest::new(
            Method::POST.as_str(),
            endpoint,
            headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            SignableBody::Bytes(body.as_bytes()),
        )
        .map_err(RsWebhookError::Signing)?;
        let mut signing_settings = SigningSettings::default();
        signing_settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
        let signing_params = SigningParams::V4(
            aws_sigv4::sign::v4::SigningParams::builder()
                .identity(identity)
                .region(region)
                .name("sts")
                .time(SystemTime::now())
                .settings(signing_settings)
                .build()
                .map_err(RsWebhookError::SigningParams)?,
        );
        let (instructions, _signature) = sign(signable_request, &signing_params)
            .map_err(RsWebhookError::Signing)?
            .into_parts();
        Ok(instructions)
    }

    /// This uses aws-sigv4 to create signed headers
    /// aws-sigv4 requires http::Request, so we build a temporary one for it to sign
    /// then read them out again to apply to the real one sent via the reqwest client
    fn signed_headers(
        instructions: SigningInstructions,
        endpoint: &str,
        unsigned_headers: &BTreeMap<String, String>,
        body_len: usize,
    ) -> RsWebhookResult<BTreeMap<String, String>> {
        let mut header_map = HeaderMap::new();
        for (key, value) in unsigned_headers {
            let name = HeaderName::from_str(key).map_err(RsWebhookError::InvalidHeaderName)?;
            let value = HeaderValue::from_str(value).map_err(RsWebhookError::InvalidHeaderValue)?;
            header_map.insert(name, value);
        }
        // Signing container. Never sent, built purely to allow aws-sigv4 to create signed headers
        let mut temp_signing_request = http::Request::builder()
            .method(http::Method::POST)
            .uri(endpoint)
            .body(())
            .map_err(RsWebhookError::Http)?;
        *temp_signing_request.headers_mut() = header_map;
        instructions.apply_to_request_http1x(&mut temp_signing_request);
        // read headers back out
        let mut signed_headers: BTreeMap<String, String> = temp_signing_request
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        signed_headers.insert(HEADER_CONTENT_LENGTH_KEY.into(), body_len.to_string());
        Ok(signed_headers)
    }

    async fn aws_login_response(
        &self,
        base_url: &str,
        headers: &BTreeMap<String, String>,
        body: &str,
        identity_id: &str,
    ) -> RsWebhookResult<InfisicalTokenResponse> {
        let iam_http_request_method = Method::POST.to_string();
        let iam_request_headers =
            BASE64.encode(serde_json::to_vec(&headers).map_err(RsWebhookError::Serde)?);
        let iam_request_body = BASE64.encode(body.as_bytes());
        let params = InfisicalAwsAuthRequest {
            identity_id: identity_id.into(),
            iam_http_request_method,
            iam_request_headers,
            iam_request_body,
        };
        let url = format!(
            "{}/{}",
            base_url.trim_end_matches("/"),
            INFISICAL_AWS_LOGIN_PATH.trim_start_matches("/")
        );
        self.http_client
            .post(url)
            .json(&params)
            .send()
            .await
            .map_err(RsWebhookError::Reqwest)?
            .error_for_status()
            .map_err(RsWebhookError::Reqwest)?
            .json::<InfisicalTokenResponse>()
            .await
            .map_err(RsWebhookError::Reqwest)
    }

    async fn secrets_response(
        &self,
        infisical_base_url: &str,
        access_token: &str,
        project_id: &str,
        environment: &str,
    ) -> RsWebhookResult<InfisicalSecretsResponse> {
        let url = format!(
            "{}/{}?projectId={}&environment={}&viewSecretValue=true",
            infisical_base_url.trim_end_matches("/"),
            INFISICAL_SECRETS_PATH.trim_start_matches("/"),
            project_id,
            environment
        );
        self.http_client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(RsWebhookError::Reqwest)?
            .error_for_status()
            .map_err(RsWebhookError::Reqwest)?
            .json::<InfisicalSecretsResponse>()
            .await
            .map_err(RsWebhookError::Reqwest)
    }
}
