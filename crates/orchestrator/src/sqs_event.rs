use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqsEvent {
    #[serde(rename = "Records")]
    pub records: Vec<SqsRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqsRecord {
    #[serde(rename = "messageId")]
    pub message_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqsEventResponse {
    #[serde(rename = "batchItemFailures")]
    pub batch_item_failures: Vec<SqsBatchItemFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqsBatchItemFailure {
    #[serde(rename = "itemIdentifier")]
    pub item_identifier: String,
}

impl SqsEventResponse {
    pub fn add_failure(&mut self, message_id: String) {
        self.batch_item_failures.push(SqsBatchItemFailure {
            item_identifier: message_id,
        });
    }
}
