use crate::{
    aws_ops::build_request::BuildRequest,
    error::{RsOrchestratorError, RsOrchestratorResult, RunTaskFailure, TaskFailure},
};
use aws_config::SdkConfig;
use aws_sdk_ecs::{
    Client as EcsClient,
    types::{AwsVpcConfiguration, LaunchType, NetworkConfiguration, TaskOverride},
};
use ref_sync_shared::{
    env_vars::EnvVars,
    errors::{config::RsConfigError, error::RsSharedError, timeout::RsTimeoutError},
    timeouts::{Deadline, Timeouts},
};

const BUILD_AGENT_CLUSTER_ARN: &str = "BUILD_AGENT_CLUSTER_ARN";
const BUILD_AGENT_CONTAINER_NAME: &str = "BUILD_AGENT_CONTAINER_NAME";
const BUILD_AGENT_SECURITY_GROUP_IDS_CSV: &str = "BUILD_AGENT_SECURITY_GROUP_IDS";
const BUILD_AGENT_SUBNET_IDS_CSV: &str = "BUILD_AGENT_SUBNET_IDS";
const BUILD_AGENT_TASK_DEFINITION_ARN: &str = "BUILD_AGENT_TASK_DEFINITION_ARN";

pub struct EcsConfig {
    pub cluster_arn: String,
    pub container_name: String,
    pub security_group_ids: Vec<String>,
    pub subnet_ids: Vec<String>,
    pub task_definition_arn: String,
}

impl EcsConfig {
    pub fn from_env() -> RsOrchestratorResult<Self> {
        let security_group_ids = EnvVars::csv_env_var(BUILD_AGENT_SECURITY_GROUP_IDS_CSV)?;
        if security_group_ids.is_empty() {
            Err(RsSharedError::Config(RsConfigError::empty(
                BUILD_AGENT_SECURITY_GROUP_IDS_CSV,
            )))?;
        }
        let subnet_ids = EnvVars::csv_env_var(BUILD_AGENT_SUBNET_IDS_CSV)?;
        if subnet_ids.is_empty() {
            Err(RsSharedError::Config(RsConfigError::empty(
                BUILD_AGENT_SUBNET_IDS_CSV,
            )))?;
        }
        Ok(Self {
            cluster_arn: EnvVars::env_var(BUILD_AGENT_CLUSTER_ARN)?,
            container_name: EnvVars::env_var(BUILD_AGENT_CONTAINER_NAME)?,
            security_group_ids,
            subnet_ids,
            task_definition_arn: EnvVars::env_var(BUILD_AGENT_TASK_DEFINITION_ARN)?,
        })
    }
}

pub struct Ecs {
    client: EcsClient,
    config: EcsConfig,
    timeouts: Timeouts,
}

impl Ecs {
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsOrchestratorResult<Self> {
        Ok(Self {
            client: EcsClient::new(sdk_config),
            config: EcsConfig::from_env()?,
            timeouts,
        })
    }
    pub async fn run_build(
        &self,
        request: BuildRequest,
        deadline: Deadline,
    ) -> RsOrchestratorResult<()> {
        let client_token = request.client_token()?;
        let container_override = request.container(&self.config.container_name);
        let overrides = TaskOverride::builder()
            .container_overrides(container_override)
            .build();
        let vpc_configuration = AwsVpcConfiguration::builder()
            .set_subnets(Some(self.config.subnet_ids.clone()))
            .set_security_groups(Some(self.config.security_group_ids.clone()))
            .build()
            .map_err(RsOrchestratorError::NetworkConfiguration)?;
        let network_configuration = NetworkConfiguration::builder()
            .awsvpc_configuration(vpc_configuration)
            .build();
        tracing::info!(
            cluster = %self.config.cluster_arn,
            task_definition = %self.config.task_definition_arn,
            "Starting build agent task"
        );
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let output = tokio::time::timeout(
            timeout,
            self.client
                .run_task()
                .cluster(self.config.cluster_arn.as_str())
                .task_definition(self.config.task_definition_arn.as_str())
                .launch_type(LaunchType::Fargate)
                .network_configuration(network_configuration)
                .overrides(overrides)
                .client_token(client_token)
                .send(),
        )
        .await
        .map_err(|_| {
            RsSharedError::Timeout(RsTimeoutError::Operation {
                operation: "ECS run_task",
            })
        })?
        .map_err(|error| Box::new(error.into_service_error()))?;
        let failures = output
            .failures()
            .iter()
            .map(|failure| TaskFailure {
                arn: failure.arn().map(str::to_owned),
                reason: failure.reason().map(str::to_owned),
                detail: failure.detail().map(str::to_owned),
            })
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(RsOrchestratorError::RunTaskFailure(
                RunTaskFailure::Failures(failures),
            ));
        }
        if output.tasks().is_empty() {
            return Err(RsOrchestratorError::RunTaskFailure(RunTaskFailure::NoTasks));
        }
        Ok(())
    }
}
