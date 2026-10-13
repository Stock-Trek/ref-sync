use ref_sync_shared::sqs_message::SqsMessage;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventResponse {
    Accept(Vec<SqsMessage>),
    Ignore,
    Pong,
    Unauthorized,
    WrongGitProvider,
    Rejected,
}
