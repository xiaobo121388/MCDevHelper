//! Shared, explicitly trusted MCDH plugins. This is not a security sandbox.

mod archive;
mod error;
mod github;
mod manifest;
mod origin;
mod runtime;
mod sdk;
mod store;

pub use error::{PluginError, Result};
pub use github::{Cancellation, PluginProgress};
pub use manifest::*;
pub use runtime::{ExecutionRequest, ExecutionResult, PluginCaller, UiAction};
pub use sdk::{PluginLog, PluginPanelInfo};
pub use store::{PluginCandidate, PluginService, PluginSummary};

#[cfg(test)]
mod engine_probe;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod runtime_tests;
