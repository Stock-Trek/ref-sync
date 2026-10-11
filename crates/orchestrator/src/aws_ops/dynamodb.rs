use aws_config::SdkConfig;
use aws_sdk_dynamodb::{
    Client as DynamoDbClient, error::SdkError, operation::put_item::PutItemError,
    types::AttributeValue,
};
use ref_sync_shared::{
    env_vars::EnvVars,
    error::{RefSyncSharedError, SharedResult},
    sqs_message::{
        GitSource, SqsDetail, SqsMessage, SqsMultiRepoAction, SqsRefAction, SqsRefType,
        SqsRepoAction,
    },
    timeouts::Timeouts,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

const DYNAMODB_DELETIONS_TABLE: &str = "DYNAMODB_DELETIONS_TABLE";
const DELETIONS_KEY_ATTRIBUTE: &str = "idempotency_key";
const DELETIONS_MESSAGE_ATTRIBUTE: &str = "message";
const DELETIONS_CREATED_AT_ATTRIBUTE: &str = "created_at";
const DELETIONS_KEY_CONDITION: &str = "attribute_not_exists(#key)";

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
        let message = SqsMessage {
            source: source.clone(),
            detail: SqsDetail::AllRepos {
                action: SqsMultiRepoAction::Remove,
            },
        };
        self.register_delete(&message, deadline).await
    }

    pub async fn register_delete_repository(
        &self,
        source: &GitSource,
        repo_id: &str,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let message = SqsMessage {
            source: source.clone(),
            detail: SqsDetail::Repo {
                action: SqsRepoAction::Remove,
                repo_id: repo_id.to_string(),
            },
        };
        self.register_delete(&message, deadline).await
    }

    pub async fn register_delete_ref(
        &self,
        source: &GitSource,
        repo_id: &str,
        ref_name: &str,
        ref_type: SqsRefType,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let message = SqsMessage {
            source: source.clone(),
            detail: SqsDetail::Ref {
                action: SqsRefAction::Delete,
                repo_id: repo_id.to_string(),
                ref_name: ref_name.to_string(),
                ref_type,
            },
        };
        self.register_delete(&message, deadline).await
    }

    async fn register_delete(
        &self,
        message: &SqsMessage,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let table = self.config.deletions_table.as_deref().ok_or_else(|| {
            RefSyncSharedError::Config(format!("{DYNAMODB_DELETIONS_TABLE} is not set"))
        })?;
        let timeout = self.timeouts.operation_for(deadline)?;
        let key = Self::message_key(message)?;
        let encoded = serde_json::to_string(message)?;
        let result = tokio::time::timeout(
            timeout,
            self.client
                .put_item()
                .table_name(table)
                .item(DELETIONS_KEY_ATTRIBUTE, AttributeValue::S(key))
                .item(DELETIONS_MESSAGE_ATTRIBUTE, AttributeValue::S(encoded))
                .item(
                    DELETIONS_CREATED_AT_ATTRIBUTE,
                    AttributeValue::N(Self::now().to_string()),
                )
                .condition_expression(DELETIONS_KEY_CONDITION)
                .expression_attribute_names("#key", DELETIONS_KEY_ATTRIBUTE)
                .send(),
        )
        .await
        .map_err(|_| RefSyncSharedError::Timeout("DynamoDB put_item timed out".into()))?;
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if Self::is_conditional_check_failed_put(&error) {
                    return Ok(());
                }
                if error.as_service_error().is_some() {
                    return Err(RefSyncSharedError::DynamoDbPutItem(Box::new(
                        error.into_service_error(),
                    )));
                }
                Err(RefSyncSharedError::InternalServer(format!(
                    "DynamoDB put_item failed: {error}"
                )))
            }
        }
    }

    fn is_conditional_check_failed_put(error: &SdkError<PutItemError>) -> bool {
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
