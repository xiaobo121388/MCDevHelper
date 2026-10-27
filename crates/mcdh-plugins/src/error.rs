use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, PluginError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginError {
    pub code: String,
    pub message: String,
}

impl PluginError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PluginError {}

impl From<std::io::Error> for PluginError {
    fn from(error: std::io::Error) -> Self {
        Self::new("io_error", error)
    }
}
impl From<serde_json::Error> for PluginError {
    fn from(error: serde_json::Error) -> Self {
        Self::new("invalid_manifest", error)
    }
}
impl From<mcdh_core::CoreError> for PluginError {
    fn from(error: mcdh_core::CoreError) -> Self {
        Self::new(error.code(), error)
    }
}

pub(crate) fn invalid(message: impl ToString) -> PluginError {
    PluginError::new("invalid_plugin", message)
}
