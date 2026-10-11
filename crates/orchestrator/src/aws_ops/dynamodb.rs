use aws_config::SdkConfig;
use aws_sdk_dynamodb::{
    Client as DynamoDbClient,
    error::SdkError,
    operation::{
        delete_item::DeleteItemError, put_item::PutItemError, update_item::UpdateItemError,
    },
};
use ref_sync_shared::{
    env_vars::EnvVars,
    error::{RefSyncSharedError, SharedResult},
    sqs_message::{GitSource, SqsRefType},
    timeouts::Timeouts,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

const DYNAMODB_DELETIONS_TABLE: &str = "DYNAMODB_DELETIONS_TABLE";

#[derive(Clone)]
pub struct DynamoDbConfig {
    pub deletions_table: Option<String>,
}

impl DynamoDbConfig {
    pub fn from_env() -> SharedResult<Self> {
        Ok(Self {
            deletions_table: EnvVars::optional_env_var(DYNAMODB_DELETIONS_TABLE),
        })
    }
}

#[derive(Clone)]
pub struct DynamoDb {
    client: DynamoDbClient,
    config: DynamoDbConfig,
    timeouts: Timeouts,
}

impl DynamoDb {
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> SharedResult<Self> {
        Ok(Self {
            client: DynamoDbClient::new(sdk_config),
            config: DynamoDbConfig::from_env()?,
            timeouts,
        })
    }

    pub async fn register_delete_installation(
        &self,
        source: &GitSource,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let table = self.config.deletions_table.as_deref().ok_or_else(|| {
            RefSyncSharedError::Config(format!("{DYNAMODB_DELETIONS_TABLE} is not set"))
        })?;
        let timeout = self.timeouts.operation_for(deadline)?;
        Ok(())
    }

    pub async fn register_delete_repository(
        &self,
        source: &GitSource,
        repo_id: &str,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let table = self.config.deletions_table.as_deref().ok_or_else(|| {
            RefSyncSharedError::Config(format!("{DYNAMODB_DELETIONS_TABLE} is not set"))
        })?;
        let timeout = self.timeouts.operation_for(deadline)?;
        Ok(())
    }

    pub async fn register_delete_ref(
        &self,
        source: &GitSource,
        repo_id: &str,
        ref_name: &str,
        ref_type: SqsRefType,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let table = self.config.deletions_table.as_deref().ok_or_else(|| {
            RefSyncSharedError::Config(format!("{DYNAMODB_DELETIONS_TABLE} is not set"))
        })?;
        let timeout = self.timeouts.operation_for(deadline)?;
        Ok(())
    }

    fn is_conditional_check_failed_put(error: &SdkError<PutItemError>) -> bool {
        error
            .as_service_error()
            .map(|error| error.is_conditional_check_failed_exception())
            .unwrap_or(false)
    }

    fn is_conditional_check_failed_update(error: &SdkError<UpdateItemError>) -> bool {
        error
            .as_service_error()
            .map(|error| error.is_conditional_check_failed_exception())
            .unwrap_or(false)
    }

    fn is_conditional_check_failed_delete(error: &SdkError<DeleteItemError>) -> bool {
        error
            .as_service_error()
            .map(|error| error.is_conditional_check_failed_exception())
            .unwrap_or(false)
    }

    fn message_key(message: &SqsMessage) -> SharedResult<String> {
        if let Some(key) = message.idempotency_key() {
            return Ok(key);
        }
        let payload = serde_json::to_vec(message)?;
        Ok(hex::encode(Sha256::digest(&payload)))
    }

    fn now() -> i64 {
        Self::to_epoch(SystemTime::now())
    }

    fn to_epoch(time: SystemTime) -> i64 {
        time.duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default()
    }
}
