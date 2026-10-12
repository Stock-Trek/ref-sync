use crate::aws_ops::tombstones::Tombstone;
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{Client as DynamoDbClient, types::AttributeValue};
use ref_sync_shared::{
    env_vars::EnvVars,
    error::{InternalServerError, RsSharedError, RsSharedResult, TimeoutError},
    timeouts::{Deadline, Timeouts},
};
use std::collections::HashMap;

const DYNAMODB_TOMBSTONE_TABLE: &str = "DYNAMODB_TOMBSTONE_TABLE";
const PK_ATTRIBUTE: &str = "pk";
const SK_ATTRIBUTE: &str = "sk";
const LAST_SEEN_AT_MILLIS_ATTRIBUTE: &str = "last_seen_at_millis";
const AUDIT_PK_ATTRIBUTE: &str = "audit_pk";
const AUDIT_SK_ATTRIBUTE: &str = "audit_sk";
const DELIVERY_COUNT_ATTRIBUTE: &str = "delivery_count";
const DELIVERY_COUNT_INCREMENT: &str = "1";

const LATEST_SIGHTING_ATTRIBUTES: [&str; 3] = [
    LAST_SEEN_AT_MILLIS_ATTRIBUTE,
    AUDIT_PK_ATTRIBUTE,
    AUDIT_SK_ATTRIBUTE,
];

#[derive(Clone)]
pub struct DynamoDbConfig {
    pub tombstone_table: String,
}

impl DynamoDbConfig {
    pub fn from_env() -> RsSharedResult<Self> {
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
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsSharedResult<Self> {
        Ok(Self {
            client: DynamoDbClient::new(sdk_config),
            config: DynamoDbConfig::from_env()?,
            timeouts,
        })
    }

    pub async fn add_tombstone(
        &self,
        tombstone: &Tombstone,
        deadline: Deadline,
    ) -> RsSharedResult<()> {
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let (update_expression, attribute_names, attribute_values) =
            Self::tombstone_update(tombstone);
        let mut builder = self
            .client
            .update_item()
            .table_name(&self.config.tombstone_table)
            .set_key(Some(tombstone.keys()))
            .update_expression(update_expression);
        for (placeholder, attribute_name) in attribute_names {
            builder = builder.expression_attribute_names(placeholder, attribute_name);
        }
        for (placeholder, attribute_value) in attribute_values {
            builder = builder.expression_attribute_values(placeholder, attribute_value);
        }
        let result = tokio::time::timeout(timeout, builder.send())
            .await
            .map_err(|_| {
                RsSharedError::Timeout(TimeoutError::Operation {
                    operation: "DynamoDB update_item",
                })
            })?;
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if error.as_service_error().is_some() {
                    return Err(RsSharedError::DynamoDbUpdateItem(Box::new(
                        error.into_service_error(),
                    )));
                }
                Err(RsSharedError::InternalServer(
                    InternalServerError::DynamoDbUpdateItem(Box::new(error)),
                ))
            }
        }
    }

    fn tombstone_update(
        tombstone: &Tombstone,
    ) -> (
        String,
        HashMap<String, String>,
        HashMap<String, AttributeValue>,
    ) {
        let mut attributes = tombstone.attributes().into_iter().collect::<Vec<_>>();
        attributes.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut set_clauses = Vec::new();
        let mut attribute_names = HashMap::new();
        let mut attribute_values = HashMap::new();

        for (index, (attribute, value)) in attributes.into_iter().enumerate() {
            if attribute == PK_ATTRIBUTE || attribute == SK_ATTRIBUTE {
                continue;
            }
            if attribute == DELIVERY_COUNT_ATTRIBUTE {
                attribute_names.insert("#delivery_count".to_string(), attribute);
                attribute_values.insert(
                    ":delivery_count_increment".to_string(),
                    AttributeValue::N(DELIVERY_COUNT_INCREMENT.to_string()),
                );
                continue;
            }

            let name = format!("#n{index}");
            let value_placeholder = format!(":v{index}");
            if LATEST_SIGHTING_ATTRIBUTES.contains(&attribute.as_str()) {
                set_clauses.push(format!("{name} = {value_placeholder}"));
            } else {
                set_clauses.push(format!(
                    "{name} = if_not_exists({name}, {value_placeholder})"
                ));
            }
            attribute_names.insert(name, attribute);
            attribute_values.insert(value_placeholder, value);
        }

        let update_expression = format!(
            "SET {} ADD #delivery_count :delivery_count_increment",
            set_clauses.join(", ")
        );
        (update_expression, attribute_names, attribute_values)
    }
}
