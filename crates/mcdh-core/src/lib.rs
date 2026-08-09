//! Shared domain services for MCDevHelper.

mod error;
mod index;
mod model;

pub use error::{CoreError, ErrorPayload, Result};
pub use index::{LocalIndex, MutationGuard};
pub use model::{
    ComponentKind, ComponentOrigin, ComponentSummary, IdentityPolicy, ManifestSummary, McsInfo,
    OperationResult, SourceKind, SourceRecord, VersionPart,
};

/// Current application version shared by desktop and protocol adapters.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
