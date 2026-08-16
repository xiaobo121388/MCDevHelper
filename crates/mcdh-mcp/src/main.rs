use std::path::PathBuf;

use mcdh_core::{
    AppSettings, BumpManifestVersionRequest, ComponentKind, ComponentService, ContentMode,
    CopyComponentRequest, CreateComponentRequest, DiscoveryService, ExportComponentRequest,
    ExportConflictPolicy, IdentityPolicy, ImportComponentRequest, LocalIndex, MoveComponentRequest,
    SetComponentMetadataRequest, SetComponentTagsRequest, SourceKind, ThemePreference, VersionPart,
};
use rmcp::{
    Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone)]
struct McdhServer {
    index: LocalIndex,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(
    name = "mcdh",
    version = "0.1.0",
    instructions = "离线管理网易中国版 Minecraft PE 创作组件；所有路径都应使用本机 Windows 绝对路径。",
    router = self.tool_router
)]
impl ServerHandler for McdhServer {}

#[tool_router(router = tool_router)]
impl McdhServer {
    fn open() -> mcdh_core::Result<Self> {
        Ok(Self {
            index: LocalIndex::open_default()?,
            tool_router: Self::tool_router(),
        })
    }

    fn service(&self) -> ComponentService {
        ComponentService::new(self.index.clone())
    }

    #[tool(description = "列出 MCDH 当前发现的全部本地组件，包括类型、路径、来源、版本和标签")]
    fn list_components(&self, Parameters(_): Parameters<EmptyParams>) -> ToolResult {
        json_result(
            DiscoveryService::new(self.index.clone())
                .refresh()
                .map(|result| result.components),
        )
    }

    #[tool(description = "按稳定组件 ID 获取一个组件的完整摘要")]
    fn get_component(&self, Parameters(params): Parameters<ComponentIdParams>) -> ToolResult {
        json_result(self.service().get_component(&params.component_id))
    }

    #[tool(description = "重新扫描已保存的 MCS 和自定义来源，并返回组件、来源及扫描警告")]
    fn refresh_components(&self, Parameters(_): Parameters<EmptyParams>) -> ToolResult {
        json_result(DiscoveryService::new(self.index.clone()).refresh())
    }

    #[tool(description = "列出已保存的 MCS 分类目录、单组件路径和组件库路径")]
    fn list_sources(&self, Parameters(_): Parameters<EmptyParams>) -> ToolResult {
        json_result(self.index.list_sources())
    }

    #[tool(description = "将指定文件夹登记为单个组件来源，不复制或删除任何磁盘内容")]
    fn add_single_component(&self, Parameters(params): Parameters<PathParams>) -> ToolResult {
        json_result(self.index.add_source(SourceKind::Single, params.path))
    }

    #[tool(description = "将指定文件夹登记为组件库，并扫描其直接子目录")]
    fn add_library(&self, Parameters(params): Parameters<PathParams>) -> ToolResult {
        json_result(self.index.add_source(SourceKind::Library, params.path))
    }

    #[tool(
        description = "保存一个 MCStudio 分类目录或其上级目录；自动展开有效的 AddOn、Map、Material、Light 目录"
    )]
    fn add_mcs_path(&self, Parameters(params): Parameters<PathParams>) -> ToolResult {
        json_result(DiscoveryService::new(self.index.clone()).add_mcs_source_path(&params.path))
    }

    #[tool(description = "重新枚举 Windows 逻辑磁盘并保存当前找到的 MCStudio 作品分类目录")]
    fn rescan_mcs_paths(&self, Parameters(_): Parameters<EmptyParams>) -> ToolResult {
        json_result(DiscoveryService::new(self.index.clone()).rescan_mcs_sources())
    }

    #[tool(description = "移除一个自定义来源登记；不会删除来源目录或其中组件")]
    fn remove_source(&self, Parameters(params): Parameters<SourceIdParams>) -> ToolResult {
        json_result(self.index.remove_source(&params.source_id))
    }

    #[tool(description = "读取本地开发者身份、默认新建目录和界面主题设置")]
    fn get_settings(&self, Parameters(_): Parameters<EmptyParams>) -> ToolResult {
        json_result(self.index.app_settings())
    }

    #[tool(description = "保存本地开发者身份、默认新建目录和界面主题设置")]
    fn set_settings(&self, Parameters(params): Parameters<SettingsParams>) -> ToolResult {
        json_result(self.index.set_app_settings(&AppSettings {
            developer_nickname: params.developer_nickname,
            developer_account: params.developer_account,
            developer_user_id: params.developer_user_id,
            default_destination: params.default_destination,
            theme: params.theme.into(),
        }))
    }

    #[tool(description = "从内置模板新建模组、材质或地图；MCS 模式会生成兼容配置和新 UID")]
    fn create_component(&self, Parameters(params): Parameters<CreateParams>) -> ToolResult {
        json_result(self.service().create_component(&CreateComponentRequest {
            name: params.name,
            kind: params.kind.into(),
            destination: params.destination,
            mcs_compatible: params.mcs_compatible,
            namespace: params.namespace,
        }))
    }

    #[tool(description = "从文件夹、ZIP、mcpack 或 mcaddon 安全导入组件")]
    fn import_component(&self, Parameters(params): Parameters<ImportParams>) -> ToolResult {
        json_result(self.service().import_component(&ImportComponentRequest {
            source: params.source,
            destination: params.destination,
            mcs_compatible: params.mcs_compatible,
            identity_policy: params.identity_policy.into(),
            content_mode: params.content_mode.into(),
        }))
    }

    #[tool(
        description = "复制组件到指定目录，可保留或重生 manifest UUID；复制到 MCS 时总会生成新 MCS UID"
    )]
    fn copy_component(&self, Parameters(params): Parameters<CopyParams>) -> ToolResult {
        json_result(self.service().copy_component(&CopyComponentRequest {
            component_id: params.component_id,
            destination: params.destination,
            mcs_compatible: params.mcs_compatible,
            identity_policy: params.identity_policy.into(),
        }))
    }

    #[tool(
        description = "移动组件到指定目录；默认保留 manifest UUID，跨盘时使用校验后的复制再移除原目录"
    )]
    fn move_component(&self, Parameters(params): Parameters<MoveParams>) -> ToolResult {
        json_result(self.service().move_component(&MoveComponentRequest {
            component_id: params.component_id,
            destination: params.destination,
            mcs_compatible: params.mcs_compatible,
        }))
    }

    #[tool(description = "将组件导出为 ZIP；可在同名文件存在时自动追加序号、覆盖或报错")]
    fn export_component(&self, Parameters(params): Parameters<ExportParams>) -> ToolResult {
        json_result(self.service().export_component(&ExportComponentRequest {
            component_id: params.component_id,
            destination: params.destination,
            content_mode: params.content_mode.into(),
            conflict_policy: params.conflict_policy.into(),
        }))
    }

    #[tool(description = "设置组件标签；MCS 组件会同时同步 work.mcscfg.CustomTags")]
    fn set_component_tags(&self, Parameters(params): Parameters<TagsParams>) -> ToolResult {
        json_result(self.service().set_component_tags(&SetComponentTagsRequest {
            component_id: params.component_id,
            tags: params.tags,
        }))
    }

    #[tool(description = "设置组件显示名称、标签和收藏状态，并写入组件根目录 .mcdh.json")]
    fn set_component_metadata(&self, Parameters(params): Parameters<MetadataParams>) -> ToolResult {
        json_result(
            self.service()
                .set_component_metadata(&SetComponentMetadataRequest {
                    component_id: params.component_id,
                    display_name: params.display_name,
                    tags: params.tags,
                    favorite: params.favorite,
                }),
        )
    }

    #[tool(
        description = "随机重生组件中所有已识别 manifest 的 header/module UUID，并同步内部依赖和地图清单"
    )]
    fn regenerate_manifest_uuids(
        &self,
        Parameters(params): Parameters<ComponentIdParams>,
    ) -> ToolResult {
        json_result(
            self.service()
                .regenerate_manifest_uuids(&params.component_id),
        )
    }

    #[tool(
        description = "提升 manifest 的 major、minor 或 patch 版本，并同步 module、内部依赖和地图清单"
    )]
    fn bump_manifest_version(&self, Parameters(params): Parameters<BumpParams>) -> ToolResult {
        json_result(
            self.service()
                .bump_manifest_version(&BumpManifestVersionRequest {
                    component_id: params.component_id,
                    part: params.part.into(),
                }),
        )
    }

    #[tool(description = "使用 Windows 文件资源管理器打开组件目录")]
    fn open_component_directory(
        &self,
        Parameters(params): Parameters<ComponentIdParams>,
    ) -> ToolResult {
        json_result(
            self.service()
                .open_component_directory(&params.component_id)
                .map(|()| ActionCompleted { completed: true }),
        )
    }

    #[tool(description = "优先用组件根目录唯一的 .code-workspace 打开 VS Code，否则打开组件目录")]
    fn open_component_in_vscode(
        &self,
        Parameters(params): Parameters<ComponentIdParams>,
    ) -> ToolResult {
        json_result(
            self.service()
                .open_component_in_vscode(&params.component_id)
                .map(|()| ActionCompleted { completed: true }),
        )
    }
}

type ToolResult = Result<Json<Value>, String>;

fn json_result<T: Serialize>(result: mcdh_core::Result<T>) -> ToolResult {
    let value = result.map_err(|error| {
        serde_json::to_string(&error.payload()).unwrap_or_else(|_| error.to_string())
    })?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| format!(r#"{{"code":"serialization_error","message":"{error}"}}"#))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComponentIdParams {
    #[schemars(description = "组件稳定 ID，可从 list_components 获得")]
    component_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SourceIdParams {
    #[schemars(description = "自定义来源 ID，可从 list_sources 获得")]
    source_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PathParams {
    #[schemars(description = "Windows 绝对目录路径")]
    path: PathBuf,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SettingsParams {
    #[schemars(description = "写入 MCS 配置的本地开发者昵称")]
    developer_nickname: String,
    #[schemars(description = "写入 MCS 配置的本地开发者账号")]
    developer_account: String,
    #[schemars(description = "写入 MCS 配置的本地用户 ID")]
    developer_user_id: String,
    #[schemars(description = "新建组件默认目录；传 null 可清除默认值")]
    default_destination: Option<PathBuf>,
    theme: McpThemePreference,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateParams {
    name: String,
    kind: McpComponentKind,
    destination: PathBuf,
    #[serde(default)]
    mcs_compatible: bool,
    #[serde(default)]
    namespace: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    source: PathBuf,
    destination: PathBuf,
    #[serde(default)]
    mcs_compatible: bool,
    #[serde(default)]
    identity_policy: McpIdentityPolicy,
    #[serde(default)]
    content_mode: McpContentMode,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CopyParams {
    component_id: String,
    destination: PathBuf,
    #[serde(default)]
    mcs_compatible: bool,
    #[serde(default)]
    identity_policy: McpCopyIdentityPolicy,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MoveParams {
    component_id: String,
    destination: PathBuf,
    #[serde(default)]
    mcs_compatible: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    component_id: String,
    destination: PathBuf,
    #[serde(default)]
    content_mode: McpContentMode,
    #[serde(default)]
    conflict_policy: McpExportConflictPolicy,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TagsParams {
    component_id: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MetadataParams {
    component_id: String,
    display_name: String,
    tags: Vec<String>,
    favorite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BumpParams {
    component_id: String,
    #[serde(default)]
    part: McpVersionPart,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpComponentKind {
    Addon,
    Material,
    Map,
}

impl From<McpComponentKind> for ComponentKind {
    fn from(value: McpComponentKind) -> Self {
        match value {
            McpComponentKind::Addon => Self::Addon,
            McpComponentKind::Material => Self::Material,
            McpComponentKind::Map => Self::Map,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpIdentityPolicy {
    Preserve,
    Regenerate,
    #[default]
    Error,
}

impl From<McpIdentityPolicy> for IdentityPolicy {
    fn from(value: McpIdentityPolicy) -> Self {
        match value {
            McpIdentityPolicy::Preserve => Self::Preserve,
            McpIdentityPolicy::Regenerate => Self::Regenerate,
            McpIdentityPolicy::Error => Self::Error,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpCopyIdentityPolicy {
    Preserve,
    #[default]
    Regenerate,
}

impl From<McpCopyIdentityPolicy> for IdentityPolicy {
    fn from(value: McpCopyIdentityPolicy) -> Self {
        match value {
            McpCopyIdentityPolicy::Preserve => Self::Preserve,
            McpCopyIdentityPolicy::Regenerate => Self::Regenerate,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpVersionPart {
    Major,
    Minor,
    #[default]
    Patch,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpContentMode {
    #[default]
    Clean,
    Full,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpExportConflictPolicy {
    #[default]
    Rename,
    Overwrite,
    Error,
}

impl From<McpContentMode> for ContentMode {
    fn from(value: McpContentMode) -> Self {
        match value {
            McpContentMode::Clean => Self::Clean,
            McpContentMode::Full => Self::Full,
        }
    }
}

impl From<McpExportConflictPolicy> for ExportConflictPolicy {
    fn from(value: McpExportConflictPolicy) -> Self {
        match value {
            McpExportConflictPolicy::Rename => Self::Rename,
            McpExportConflictPolicy::Overwrite => Self::Overwrite,
            McpExportConflictPolicy::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum McpThemePreference {
    Light,
    Dark,
    System,
}

impl From<McpThemePreference> for ThemePreference {
    fn from(value: McpThemePreference) -> Self {
        match value {
            McpThemePreference::Light => Self::Light,
            McpThemePreference::Dark => Self::Dark,
            McpThemePreference::System => Self::System,
        }
    }
}

impl From<McpVersionPart> for VersionPart {
    fn from(value: McpVersionPart) -> Self {
        match value {
            McpVersionPart::Major => Self::Major,
            McpVersionPart::Minor => Self::Minor,
            McpVersionPart::Patch => Self::Patch,
        }
    }
}

#[derive(Serialize)]
struct ActionCompleted {
    completed: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = McdhServer::open()?;
    eprintln!("MCDH MCP {} started on stdio", mcdh_core::VERSION);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
