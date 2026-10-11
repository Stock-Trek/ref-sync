use crate::{aws_ops::aws::Aws, error::RsOrchestratorResult};

pub struct GwhContext {
    pub aws: Aws,
}

impl GwhContext {
    pub async fn new() -> RsOrchestratorResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        Ok(Self { aws })
    }
}
