use std::fmt;

use serde::{Deserialize, Serialize};

use crate::github::Repository;

// The GitHub representation stays compatible with existing registry records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum PluginOrigin {
    GitHub(Repository),
    Managed { kind: ManagedOrigin },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManagedOrigin {
    Local,
    BundledMcde,
}

impl From<Repository> for PluginOrigin {
    fn from(repository: Repository) -> Self {
        Self::GitHub(repository)
    }
}

impl fmt::Display for PluginOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub(repository) => repository.fmt(f),
            Self::Managed {
                kind: ManagedOrigin::Local,
            } => f.write_str("Local import"),
            Self::Managed {
                kind: ManagedOrigin::BundledMcde,
            } => f.write_str("MCDH"),
        }
    }
}
