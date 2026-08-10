//! Shared domain services for MCDevHelper.

mod archive;
mod discovery;
mod error;
mod index;
mod json;
mod model;
mod operations;
mod path_utils;
mod template;

pub use error::{CoreError, ErrorPayload, Result};
pub use index::{LocalIndex, MutationGuard};
pub use model::{
    AppSettings, BumpManifestVersionRequest, ComponentKind, ComponentOrigin, ComponentSummary,
    CopyComponentRequest, CreateComponentRequest, DiscoveryResult, DiscoveryWarning,
    ExportComponentRequest, IdentityPolicy, ImportComponentRequest, ManifestSummary, McsInfo,
    McsTemplateIdentity, MoveComponentRequest, OperationResult, SetComponentTagsRequest,
    SourceKind, SourceRecord, ThemePreference, VersionPart, VsCodeStatus,
};
pub use operations::ComponentService;
pub use template::{RenderedFile, RenderedTemplate, TemplateRequest, TemplateService};

/// Current application version shared by desktop and protocol adapters.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub use discovery::DiscoveryService;
