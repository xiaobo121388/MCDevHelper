use crate::{ExportConflictPolicy, OperationResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    Desktop,
    Mcp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartCustomExportRequest {
    pub component_id: String,
    pub profile_id: String,
    pub destination: PathBuf,
    #[serde(default = "error_policy")]
    pub conflict_policy: ExportConflictPolicy,
}

fn error_policy() -> ExportConflictPolicy {
    ExportConflictPolicy::Error
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Preparing,
    Running,
    Validating,
    AwaitingConflict,
    Publishing,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportLog {
    pub sequence: u64,
    pub source: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskFailure {
    pub code: String,
    pub message: String,
    pub exit_code: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomExportTask {
    pub id: String,
    pub component_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub destination: PathBuf,
    pub status: TaskStatus,
    pub cancel_requested: bool,
    pub conflict_path: Option<PathBuf>,
    pub result: Option<OperationResult>,
    pub error: Option<TaskFailure>,
    pub logs: Vec<ExportLog>,
    pub next_cursor: u64,
    pub logs_truncated: bool,
}
