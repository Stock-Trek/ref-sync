use crate::aws_ops::tombstones::Tombstone;
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{
    Client as DynamoDbClient, error::SdkError, operation::put_item::PutItemError,
};
use ref_sync_shared::{
    env_vars::EnvVars,
    error::{RefSyncSharedError, SharedResult},
    timeouts::Timeouts,
};
use std::time::SystemTime;

const DYNAMODB_TOMBSTONE_TABLE: &str = "DYNAMODB_TOMBSTONE_TABLE";

#[derive(Clone)]
pub struct DynamoDbConfig {
    pub tombstone_table: String,
}

impl DynamoDbConfig {
    pub fn from_env() -> SharedResult<Self> {
        Ok(Self {
            tombstone_table: EnvVars::env_var(DYNAMODB_TOMBSTONE_TABLE)?,
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

    pub async fn add_tombstone(
        &self,
        tombstone: &Tombstone,
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let timeout = self.timeouts.operation_for(deadline)?;
        let mut builder = self
            .client
            .put_item()
            .table_name(&self.config.tombstone_table);
        for (attribute, value) in tombstone.attributes() {
            builder = builder.item(attribute, value);
        }
        for condition in tombstone.condition_expressions() {
            builder = builder.condition_expression(condition);
        }
        for (expression, attribute_name) in tombstone.expressions_attribute_names() {
            builder = builder.expression_attribute_names(expression, attribute_name);
        }
        let result = tokio::time::timeout(timeout, builder.send())
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
}
