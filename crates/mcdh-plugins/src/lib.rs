//! Shared, explicitly trusted MCDH plugins. This is not a security sandbox.

mod archive;
mod error;
mod github;
mod manifest;
mod store;

pub use error::{PluginError, Result};
pub use github::{Cancellation, PluginProgress};
pub use manifest::*;
pub use store::{PluginCandidate, PluginService, PluginSummary};

#[cfg(test)]
mod engine_probe;

#[cfg(test)]
mod tests;
