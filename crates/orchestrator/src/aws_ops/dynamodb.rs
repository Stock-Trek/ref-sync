use crate::aws_ops::tombstones::Tombstone;
use aws_config::SdkConfig;
use aws_sdk_dynamodb::{Client as DynamoDbClient, types::AttributeValue};
use ref_sync_shared::{
    env_vars::EnvVars,
    error::{InternalServerError, RefSyncSharedError, SharedResult, TimeoutError},
    timeouts::Timeouts,
};
use std::{collections::HashMap, time::SystemTime};

const DYNAMODB_TOMBSTONE_TABLE: &str = "DYNAMODB_TOMBSTONE_TABLE";
const PK_ATTRIBUTE: &str = "pk";
const SK_ATTRIBUTE: &str = "sk";
const LAST_SEEN_AT_MILLIS_ATTRIBUTE: &str = "last_seen_at_millis";
const DELIVERY_COUNT_ATTRIBUTE: &str = "delivery_count";
const DELIVERY_COUNT_INCREMENT: &str = "1";

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
                RefSyncSharedError::Timeout(TimeoutError::Operation {
                    operation: "DynamoDB update_item",
                })
            })?;
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if error.as_service_error().is_some() {
                    return Err(RefSyncSharedError::DynamoDbUpdateItem(Box::new(
                        error.into_service_error(),
                    )));
                }
                Err(RefSyncSharedError::InternalServer(
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
            if attribute == LAST_SEEN_AT_MILLIS_ATTRIBUTE {
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

#[cfg(test)]
mod tests {
    use super::{AttributeValue, DynamoDb, LAST_SEEN_AT_MILLIS_ATTRIBUTE};
    use crate::aws_ops::tombstones::{DetectedVia, Reason, ScopeType, Tombstone, TombstoneContext};
    use ref_sync_shared::sqs_message::GitSource;
    use std::collections::HashMap;

    fn tombstone() -> Tombstone {
        let source = GitSource::GitHub {
            host: "github.com".to_string(),
            delivery_id: None,
            installation_id: 42,
        };
        let ctx = TombstoneContext::new(&source, Reason::ScopeDeleted, DetectedVia::Webhook);
        Tombstone::scope(&ctx, ScopeType::Installation, "42".to_string())
    }

    fn placeholder_for<'a>(names: &'a HashMap<String, String>, attribute: &str) -> &'a str {
        names
            .iter()
            .find(|(_, name)| name.as_str() == attribute)
            .map(|(placeholder, _)| placeholder.as_str())
            .unwrap_or_else(|| panic!("missing attribute {attribute}"))
    }

    #[test]
    fn keys_are_pk_and_sk() {
        let keys = tombstone().keys();
        assert!(keys.contains_key("pk"));
        assert!(keys.contains_key("sk"));
    }

    #[test]
    fn last_seen_is_always_updated() {
        let tombstone = tombstone();
        let (expression, names, _) = DynamoDb::tombstone_update(&tombstone);
        let name = placeholder_for(&names, LAST_SEEN_AT_MILLIS_ATTRIBUTE);
        let value = name.replacen("#n", ":v", 1);
        assert!(expression.contains(&format!("{name} = {value}")));
        assert!(!expression.contains(&format!("if_not_exists({name}")));
    }

    #[test]
    fn creation_attributes_are_write_once() {
        let tombstone = tombstone();
        let (expression, names, _) = DynamoDb::tombstone_update(&tombstone);
        let name = placeholder_for(&names, "deleted_at_millis");
        assert!(expression.contains(&format!("{name} = if_not_exists({name},")));
    }

    #[test]
    fn delivery_count_is_incremented() {
        let tombstone = tombstone();
        let (expression, names, values) = DynamoDb::tombstone_update(&tombstone);
        assert!(expression.contains("ADD #delivery_count :delivery_count_increment"));
        assert_eq!(
            names.get("#delivery_count"),
            Some(&"delivery_count".to_string())
        );
        assert_eq!(
            values.get(":delivery_count_increment"),
            Some(&AttributeValue::N("1".to_string()))
        );
    }
}
