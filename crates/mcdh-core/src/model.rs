use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    Addon,
    Material,
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    McsAuto,
    Single,
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityPolicy {
    Preserve,
    Regenerate,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionPart {
    Major,
    Minor,
    Patch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    pub id: String,
    pub kind: SourceKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ComponentOrigin {
    Mcs { source_path: PathBuf },
    Single { source_id: String },
    Library { source_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McsInfo {
    pub uid: String,
    pub component_type: i64,
    pub account: Option<String>,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSummary {
    pub path: PathBuf,
    pub name: Option<String>,
    pub header_uuid: Option<String>,
    pub version: Option<[u64; 3]>,
    pub module_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentSummary {
    pub id: String,
    pub name: String,
    pub kind: ComponentKind,
    pub path: PathBuf,
    pub origin: ComponentOrigin,
    pub mcs: Option<McsInfo>,
    pub manifests: Vec<ManifestSummary>,
    pub version: Option<[u64; 3]>,
    pub tags: Vec<String>,
    pub icon_path: Option<PathBuf>,
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResult {
    pub component: Option<ComponentSummary>,
    pub actual_path: PathBuf,
    pub modified_files: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateComponentRequest {
    pub name: String,
    pub kind: ComponentKind,
    pub destination: PathBuf,
    pub mcs_compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyComponentRequest {
    pub component_id: String,
    pub destination: PathBuf,
    pub mcs_compatible: bool,
    pub identity_policy: IdentityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveComponentRequest {
    pub component_id: String,
    pub destination: PathBuf,
    pub mcs_compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportComponentRequest {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub mcs_compatible: bool,
    pub identity_policy: IdentityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportComponentRequest {
    pub component_id: String,
    pub destination: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub components: Vec<ComponentSummary>,
    pub sources: Vec<SourceRecord>,
    pub warnings: Vec<DiscoveryWarning>,
}
