//! Shared domain services for MCDevHelper.

mod archive;
mod discovery;
mod error;
mod index;
mod model;
mod operations;
mod template;

pub use error::{CoreError, ErrorPayload, Result};
pub use index::{LocalIndex, MutationGuard};
pub use model::{
    ComponentKind, ComponentOrigin, ComponentSummary, CopyComponentRequest, CreateComponentRequest,
    DiscoveryResult, DiscoveryWarning, ExportComponentRequest, IdentityPolicy,
    ImportComponentRequest, ManifestSummary, McsInfo, MoveComponentRequest, OperationResult,
    SourceKind, SourceRecord, VersionPart,
};
pub use operations::ComponentService;
pub use template::{RenderedFile, RenderedTemplate, TemplateRequest, TemplateService};

/// Current application version shared by desktop and protocol adapters.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub use discovery::DiscoveryService;
