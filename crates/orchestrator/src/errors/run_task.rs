use ref_sync_shared::errors::retryability::Retryability;

#[derive(Debug, thiserror::Error)]
pub enum RsRunTaskError {
    #[error("RunTask returned no tasks")]
    NoTasks,
    #[error("RunTask reported {} failure(s): {}", .0.len(), Self::format_failures(.0))]
    Failures(Vec<TaskFailure>),
}

impl RsRunTaskError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::NoTasks => Retryability::Retryable,
            Self::Failures(failures)
                if !failures.is_empty() && failures.iter().all(TaskFailure::is_permanent) =>
            {
                Retryability::Permanent
            }
            Self::Failures(_) => Retryability::Retryable,
        }
    }

    fn format_failures(failures: &[TaskFailure]) -> String {
        failures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[derive(Debug)]
pub struct TaskFailure {
    pub arn: Option<String>,
    pub reason: Option<String>,
    pub detail: Option<String>,
}

impl TaskFailure {
    fn is_permanent(&self) -> bool {
        self.reason
            .as_deref()
            .is_some_and(Self::is_permanent_run_task_failure_reason)
    }

    fn is_permanent_run_task_failure_reason(reason: &str) -> bool {
        matches!(
            reason,
            "MISSING" | "INACTIVE" | "ATTRIBUTE" | "LOCATION" | "RESOURCE:RESOURCE_NOT_FOUND"
        )
    }
}

impl std::fmt::Display for TaskFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {} ({})",
            self.arn.as_deref().unwrap_or("unknown task"),
            self.reason.as_deref().unwrap_or("unknown reason"),
            self.detail.as_deref().unwrap_or("no detail"),
        )
    }
}
