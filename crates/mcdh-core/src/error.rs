use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("找不到路径：{0}")]
    NotFound(PathBuf),
    #[error("无效参数：{0}")]
    InvalidInput(String),
    #[error("无法识别组件：{0}")]
    InvalidComponent(PathBuf),
    #[error("组件修改正被另一个 MCDH 进程占用")]
    Busy,
    #[error("组件标识冲突：{0}")]
    Conflict(String),
    #[error("压缩包处理失败：{path}: {message}")]
    Archive { path: PathBuf, message: String },
    #[error("文件操作失败：{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON 解析失败：{path}: {message}")]
    Json { path: PathBuf, message: String },
    #[error("本地索引失败：{0}")]
    Database(#[from] rusqlite::Error),
}

impl CoreError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn json(path: impl AsRef<Path>, error: impl std::fmt::Display) -> Self {
        Self::Json {
            path: path.as_ref().to_path_buf(),
            message: error.to_string(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::InvalidInput(_) => "invalid_input",
            Self::InvalidComponent(_) => "invalid_component",
            Self::Busy => "busy",
            Self::Conflict(_) => "conflict",
            Self::Archive { .. } => "archive_error",
            Self::Io { .. } => "io_error",
            Self::Json { .. } => "json_error",
            Self::Database(_) => "database_error",
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::NotFound(path)
            | Self::InvalidComponent(path)
            | Self::Io { path, .. }
            | Self::Json { path, .. } => Some(path),
            Self::Archive { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code(),
            message: self.to_string(),
            path: self.path().map(Path::to_path_buf),
            details: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}
