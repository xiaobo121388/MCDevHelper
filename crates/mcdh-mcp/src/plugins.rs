use std::{sync::Arc, time::Duration};

use mcdh_plugins::{Cancellation, ExecutionRequest, PluginCaller, PluginError, PluginService};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::tool::ToolCallContext,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, RequestContext},
};
use serde_json::{Value, json};

use crate::McdhServer;

struct PluginRoute {
    plugin_id: String,
    fingerprint: String,
    command: String,
    tool: Tool,
}

fn routes(service: &PluginService) -> mcdh_plugins::Result<Vec<PluginRoute>> {
    let mut routes = Vec::new();
    for plugin in service.list()? {
        if !plugin.enabled || !plugin.trusted || !plugin.compatible || !plugin.allow_mcp {
            continue;
        }
        plugin.manifest.validate()?;
        for declaration in &plugin.manifest.mcp_tools {
            let command = plugin
                .manifest
                .commands
                .iter()
                .find(|command| command.id == declaration.command)
                .ok_or_else(|| PluginError::new("invalid_manifest", "Unresolved MCP command"))?;
            let schema = command.input_schema.as_object().ok_or_else(|| {
                PluginError::new(
                    "invalid_manifest",
                    "MCP tool input must be an object schema",
                )
            })?;
            let mut tool = Tool::new(
                plugin.manifest.tool_name(declaration),
                declaration.description.clone(),
                Arc::new(schema.clone()),
            );
            if let Some(output) = &command.output_schema {
                let schema = result_schema(output, &plugin.manifest.tool_name(declaration));
                tool = tool.with_raw_output_schema(Arc::new(schema));
            }
            routes.push(PluginRoute {
                plugin_id: plugin.manifest.id.clone(),
                fingerprint: plugin.fingerprint.clone(),
                command: command.id.clone(),
                tool,
            });
        }
    }
    Ok(routes)
}

fn result_schema(output: &Value, name: &str) -> serde_json::Map<String, Value> {
    let mut embedded = output.clone();
    if let Some(object) = embedded.as_object_mut() {
        // A resource boundary keeps local refs rooted in the plugin's output, not the wrapper.
        object
            .entry("$id")
            .or_insert_with(|| json!(format!("urn:mcdh:result:{name}")));
    }
    let mut schema = json!({"type":"object","properties":{"result":embedded},"required":["result"],"additionalProperties":false});
    if let Some(dialect) = output.get("$schema") {
        schema["$schema"] = dialect.clone();
    }
    schema.as_object().cloned().expect("object schema literal")
}

impl ServerHandler for McdhServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().enable_tool_list_changed().build())
            .with_server_info(Implementation::new("mcdh", mcdh_core::VERSION))
            .with_instructions("离线管理网易中国版 Minecraft PE 创作组件；所有路径都应使用本机 Windows 绝对路径。plugin__ 开头的工具由用户在桌面端单独授权的全权限插件提供。")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned().or_else(|| {
            routes(&self.plugins)
                .ok()?
                .into_iter()
                .find(|route| route.tool.name == name)
                .map(|route| route.tool)
        })
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools = self.tool_router.list_all();
        let plugins = routes(&self.plugins)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        tools.extend(plugins.into_iter().map(|route| route.tool));
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if self.tool_router.has_route(&request.name) {
            return self
                .tool_router
                .call(ToolCallContext::new(self, request, context))
                .await;
        }
        let route = match routes(&self.plugins) {
            Ok(routes) => routes
                .into_iter()
                .find(|route| route.tool.name == request.name),
            Err(error) => return Ok(plugin_error(error)),
        };
        let Some(route) = route else {
            return Ok(plugin_error(PluginError::new(
                "tool_not_authorized",
                "Unknown or unauthorized plugin tool; enable and authorize its exact version in the desktop application",
            )));
        };
        let cancellation = Cancellation::default();
        let execution = self.plugins.execute(
            ExecutionRequest {
                plugin_id: route.plugin_id,
                fingerprint: Some(route.fingerprint),
                command: route.command,
                input: Value::Object(request.arguments.unwrap_or_default()),
                component_id: None,
                timeout_ms: 60_000,
            },
            PluginCaller::Mcp,
            cancellation.clone(),
        );
        let result = tokio::select! {
            result = execution => result,
            () = context.ct.cancelled() => { cancellation.cancel(); Err(PluginError::new("cancelled", "MCP plugin request cancelled")) }
        };
        Ok(match result {
            Ok(result) => CallToolResult::structured(json!({"result":result.value})).into(),
            Err(error) => plugin_error(error),
        })
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let service = self.plugins.clone();
        let peer = context.peer;
        tokio::spawn(async move {
            let mut previous = service.revision().unwrap_or_default();
            while !peer.is_transport_closed() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                match service.revision() {
                    Ok(revision) if revision != previous => {
                        previous = revision;
                        if let Err(error) = peer.notify_tool_list_changed().await {
                            eprintln!("Plugin tool list notification: {error}");
                        }
                    }
                    Err(error) => eprintln!("Plugin registry refresh: {error}"),
                    _ => {}
                }
            }
        });
    }
}

fn plugin_error(error: PluginError) -> CallToolResponse {
    CallToolResult::error(vec![ContentBlock::text(
        serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()),
    )])
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_output_retains_local_reference_scope() {
        let output = json!({"$defs":{"number":{"type":"integer"}},"type":"object","properties":{"count":{"$ref":"#/$defs/number"}},"required":["count"]});
        let schema = Value::Object(result_schema(&output, "plugin__example__count"));
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&json!({"result":{"count":3}})));
        assert!(!validator.is_valid(&json!({"result":{"count":"invalid"}})));
        assert!(!validator.is_valid(&json!({"count":3})));
    }
}
