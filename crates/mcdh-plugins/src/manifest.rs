use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Result, error::invalid};

pub const MANIFEST_FILE: &str = "mcdh-plugin.json";
pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub schema_version: u32,
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub engines: PluginEngines,
    pub entry: String,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub panels: Vec<PluginPanel>,
    #[serde(default)]
    pub mcp_tools: Vec<PluginTool>,
    #[serde(default = "object_schema")]
    pub settings_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginEngines {
    pub mcdh: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub menus: Vec<CommandMenu>,
    #[serde(default = "object_schema")]
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommandMenu {
    Global,
    Component,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginPanel {
    pub id: String,
    pub title: String,
    pub entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    pub command: String,
}

pub fn object_schema() -> Value {
    json!({"type":"object", "properties":{}, "additionalProperties":false})
}

pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
        && !value.contains("..")
        && !value.contains("__")
}

impl PluginManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.api_version != API_VERSION {
            return Err(invalid("Unsupported plugin schema_version or api_version"));
        }
        if !identifier(&self.id)
            || self.id == "registry"
            || self.name.trim().is_empty()
            || self.name.len() > 256
        {
            return Err(invalid("Invalid plugin id or name"));
        }
        crate::archive::relative_path(&self.id)?;
        semver::Version::parse(&self.version)
            .map_err(|e| invalid(format!("Invalid plugin version: {e}")))?;
        semver::VersionReq::parse(&self.engines.mcdh)
            .map_err(|e| invalid(format!("Invalid engines.mcdh: {e}")))?;
        crate::archive::relative_path(&self.entry)?;
        let mut commands = HashSet::new();
        for command in &self.commands {
            if !identifier(&command.id)
                || command.title.trim().is_empty()
                || !commands.insert(&command.id)
            {
                return Err(invalid("Invalid or duplicate command"));
            }
            validate_schema(&command.input_schema, true)?;
            if let Some(schema) = &command.output_schema {
                validate_schema(schema, false)?;
            }
        }
        let mut panels = HashSet::new();
        for panel in &self.panels {
            if !identifier(&panel.id) || panel.title.trim().is_empty() || !panels.insert(&panel.id)
            {
                return Err(invalid("Invalid or duplicate panel"));
            }
            crate::archive::relative_path(&panel.entry)?;
        }
        let mut tools = HashSet::new();
        for tool in &self.mcp_tools {
            if !identifier(&tool.name)
                || self.tool_name(tool).len() > 128
                || tool.description.trim().is_empty()
                || !tools.insert(&tool.name)
                || !commands.contains(&tool.command)
            {
                return Err(invalid("Invalid, duplicate or unresolved MCP tool"));
            }
        }
        validate_schema(&self.settings_schema, true)?;
        validate_value(&self.settings_schema, &self.default_settings())
    }

    pub fn compatible(&self) -> bool {
        semver::VersionReq::parse(&self.engines.mcdh).is_ok_and(|range| {
            semver::Version::parse(mcdh_core::VERSION).is_ok_and(|version| range.matches(&version))
        })
    }

    pub fn tool_name(&self, tool: &PluginTool) -> String {
        format!("plugin__{}__{}", self.id, tool.name)
    }

    pub fn default_settings(&self) -> Value {
        let mut defaults = serde_json::Map::new();
        if let Some(properties) = self
            .settings_schema
            .get("properties")
            .and_then(Value::as_object)
        {
            for (name, schema) in properties {
                if let Some(default) = schema.get("default") {
                    defaults.insert(name.clone(), default.clone());
                }
            }
        }
        Value::Object(defaults)
    }
}

fn local_refs_only(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().all(|(key, value)| {
            (!matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef")
                || value
                    .as_str()
                    .is_some_and(|reference| reference.starts_with('#')))
                && local_refs_only(value)
        }),
        Value::Array(array) => array.iter().all(local_refs_only),
        _ => true,
    }
}

fn validate_schema(schema: &Value, object: bool) -> Result<()> {
    if !local_refs_only(schema)
        || (object && schema.get("type").and_then(Value::as_str) != Some("object"))
    {
        return Err(invalid(
            "Schema must be an object schema and use local references only",
        ));
    }
    jsonschema::validator_for(schema).map_err(|e| invalid(format!("Invalid JSON Schema: {e}")))?;
    Ok(())
}

pub(crate) fn validate_value(schema: &Value, value: &Value) -> Result<()> {
    let validator = jsonschema::validator_for(schema).map_err(invalid)?;
    validator
        .validate(value)
        .map_err(|e| crate::PluginError::new("invalid_arguments", e))
}
