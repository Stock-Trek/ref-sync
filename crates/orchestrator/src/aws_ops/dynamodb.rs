use crate::error::{RsOrchestratorError, RsOrchestratorResult};
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{
    Client as DynamoDbClient,
    error::SdkError,
    operation::{
        delete_item::DeleteItemError, put_item::PutItemError, update_item::UpdateItemError,
    },
    types::AttributeValue,
};
use ref_sync_shared::{env_vars::EnvVars, timeouts::Timeouts};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const IDEMPOTENCY_KEY_ATTRIBUTE: &str = "id";
const IDEMPOTENCY_STATUS_ATTRIBUTE: &str = "status";
const IDEMPOTENCY_LEASE_EXPIRES_AT_ATTRIBUTE: &str = "lease_expires_at";
const IDEMPOTENCY_EXPIRES_AT_ATTRIBUTE: &str = "expires_at";
const IDEMPOTENCY_STATUS_IN_PROGRESS: &str = "in_progress";
const IDEMPOTENCY_STATUS_DONE: &str = "done";
const IDEMPOTENCY_LEASE_TTL: Duration = Duration::from_secs(60);
const IDEMPOTENCY_DONE_TTL: Duration = Duration::from_secs(5 * 60);

const DYNAMODB_IDEMPOTENCY_TABLE: &str = "DYNAMODB_IDEMPOTENCY_TABLE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyClaim {
    Acquired { lease_expires_at: i64 },
    AlreadyDone,
    InProgress,
}

#[derive(Clone)]
pub struct DynamoDbConfig {
    pub idempotency_table: String,
}

impl DynamoDbConfig {
    pub fn from_env() -> RsOrchestratorResult<Self> {
        Ok(Self {
            idempotency_table: EnvVars::env_var(DYNAMODB_IDEMPOTENCY_TABLE)?,
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
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsOrchestratorResult<Self> {
        Ok(Self {
            client: DynamoDbClient::new(sdk_config),
            config: DynamoDbConfig::from_env()?,
            timeouts,
        })
    }

    pub async fn claim_idempotency(
        &self,
        key: &str,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<IdempotencyClaim> {
        let lease_expires_at = Self::lease_expires_at(deadline);
        let timeout = self.timeouts.operation_for(deadline)?;
        let result = tokio::time::timeout(
            timeout,
            self.client
                .put_item()
                .table_name(&self.config.idempotency_table)
                .item(IDEMPOTENCY_KEY_ATTRIBUTE, AttributeValue::S(key.to_owned()))
                .item(
                    IDEMPOTENCY_STATUS_ATTRIBUTE,
                    AttributeValue::S(IDEMPOTENCY_STATUS_IN_PROGRESS.to_owned()),
                )
                .item(
                    IDEMPOTENCY_LEASE_EXPIRES_AT_ATTRIBUTE,
                    AttributeValue::N(lease_expires_at.to_string()),
                )
                .item(
                    IDEMPOTENCY_EXPIRES_AT_ATTRIBUTE,
                    AttributeValue::N(Self::expires_at(IDEMPOTENCY_DONE_TTL).to_string()),
                )
                .condition_expression(
                    "attribute_not_exists(#key) OR (#status = :in_progress AND #lease < :now)",
                )
                .expression_attribute_names("#key", IDEMPOTENCY_KEY_ATTRIBUTE)
                .expression_attribute_names("#status", IDEMPOTENCY_STATUS_ATTRIBUTE)
                .expression_attribute_names("#lease", IDEMPOTENCY_LEASE_EXPIRES_AT_ATTRIBUTE)
                .expression_attribute_values(
                    ":in_progress",
                    AttributeValue::S(IDEMPOTENCY_STATUS_IN_PROGRESS.to_owned()),
                )
                .expression_attribute_values(":now", AttributeValue::N(Self::now().to_string()))
                .send(),
        )
        .await
        .map_err(|_| RsOrchestratorError::Timeout("DynamoDB put_item timed out".into()))?;
        match result {
            Ok(_) => Ok(IdempotencyClaim::Acquired { lease_expires_at }),
            Err(error) if Self::is_conditional_check_failed_put(&error) => {
                self.current_claim(key, deadline).await
            }
            Err(error) => Err(RsOrchestratorError::DynamoDbPutItem(Box::new(
                error.into_service_error(),
            ))),
        }
    }

    pub async fn complete_idempotency(
        &self,
        key: &str,
        lease_expires_at: i64,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<()> {
        let timeout = self.timeouts.operation_for(deadline)?;
        let result = tokio::time::timeout(
            timeout,
            self.client
                .update_item()
                .table_name(&self.config.idempotency_table)
                .key(IDEMPOTENCY_KEY_ATTRIBUTE, AttributeValue::S(key.to_owned()))
                .update_expression("SET #status = :done, #expires_at = :expires_at")
                .condition_expression("#status = :in_progress AND #lease = :lease")
                .expression_attribute_names("#status", IDEMPOTENCY_STATUS_ATTRIBUTE)
                .expression_attribute_names("#lease", IDEMPOTENCY_LEASE_EXPIRES_AT_ATTRIBUTE)
                .expression_attribute_names("#expires_at", IDEMPOTENCY_EXPIRES_AT_ATTRIBUTE)
                .expression_attribute_values(
                    ":done",
                    AttributeValue::S(IDEMPOTENCY_STATUS_DONE.to_owned()),
                )
                .expression_attribute_values(
                    ":in_progress",
                    AttributeValue::S(IDEMPOTENCY_STATUS_IN_PROGRESS.to_owned()),
                )
                .expression_attribute_values(
                    ":expires_at",
                    AttributeValue::N(Self::expires_at(IDEMPOTENCY_DONE_TTL).to_string()),
                )
                .expression_attribute_values(
                    ":lease",
                    AttributeValue::N(lease_expires_at.to_string()),
                )
                .send(),
        )
        .await
        .map_err(|_| RsOrchestratorError::Timeout("DynamoDB update_item timed out".into()))?;
        match result {
            Ok(_) => Ok(()),
            Err(error) if Self::is_conditional_check_failed_update(&error) => Ok(()),
            Err(error) => Err(RsOrchestratorError::DynamoDbUpdateItem(Box::new(
                error.into_service_error(),
            ))),
        }
    }

    pub async fn release_idempotency(
        &self,
        key: &str,
        lease_expires_at: i64,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<()> {
        let timeout = self.timeouts.operation_for(deadline)?;
        let result = tokio::time::timeout(
            timeout,
            self.client
                .delete_item()
                .table_name(&self.config.idempotency_table)
                .key(IDEMPOTENCY_KEY_ATTRIBUTE, AttributeValue::S(key.to_owned()))
                .condition_expression("#status = :in_progress AND #lease = :lease")
                .expression_attribute_names("#status", IDEMPOTENCY_STATUS_ATTRIBUTE)
                .expression_attribute_names("#lease", IDEMPOTENCY_LEASE_EXPIRES_AT_ATTRIBUTE)
                .expression_attribute_values(
                    ":in_progress",
                    AttributeValue::S(IDEMPOTENCY_STATUS_IN_PROGRESS.to_owned()),
                )
                .expression_attribute_values(
                    ":lease",
                    AttributeValue::N(lease_expires_at.to_string()),
                )
                .send(),
        )
        .await
        .map_err(|_| RsOrchestratorError::Timeout("DynamoDB delete_item timed out".into()))?;
        match result {
            Ok(_) => Ok(()),
            Err(error) if Self::is_conditional_check_failed_delete(&error) => Ok(()),
            Err(error) => Err(RsOrchestratorError::DynamoDbDeleteItem(Box::new(
                error.into_service_error(),
            ))),
        }
    }

    async fn current_claim(
        &self,
        key: &str,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<IdempotencyClaim> {
        let timeout = self.timeouts.operation_for(deadline)?;
        let output = tokio::time::timeout(
            timeout,
            self.client
                .get_item()
                .table_name(&self.config.idempotency_table)
                .key(IDEMPOTENCY_KEY_ATTRIBUTE, AttributeValue::S(key.to_owned()))
                .consistent_read(true)
                .send(),
        )
        .await
        .map_err(|_| RsOrchestratorError::Timeout("DynamoDB get_item timed out".into()))?
        .map_err(|error| {
            RsOrchestratorError::DynamoDbGetItem(Box::new(error.into_service_error()))
        })?;
        let status = output
            .item()
            .and_then(|item| item.get(IDEMPOTENCY_STATUS_ATTRIBUTE))
            .and_then(|value| value.as_s().ok())
            .map(String::as_str);
        if status == Some(IDEMPOTENCY_STATUS_DONE) {
            Ok(IdempotencyClaim::AlreadyDone)
        } else {
            Ok(IdempotencyClaim::InProgress)
        }
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

    fn now() -> i64 {
        Self::to_epoch(SystemTime::now())
    }

    fn expires_at(ttl: Duration) -> i64 {
        Self::now() + ttl.as_secs() as i64
    }

    fn lease_expires_at(deadline: SystemTime) -> i64 {
        let lease = Self::now() + IDEMPOTENCY_LEASE_TTL.as_secs() as i64;
        Self::to_epoch(deadline).min(lease)
    }

    fn to_epoch(time: SystemTime) -> i64 {
        time.duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default()
    }
}
