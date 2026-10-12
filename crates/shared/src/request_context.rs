use crate::timeouts::Deadline;

#[derive(Debug, Clone)]
pub struct RequestContext<Context> {
    pub context: Context,
    pub deadline: Deadline,
}
