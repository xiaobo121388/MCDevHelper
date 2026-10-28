import type { Json, PluginEnvironment } from "../sdk/mcdh";

export type { Json };
export type JsonSchema = { type?: string; title?: string; description?: string; properties?: Record<string, JsonSchema>; required?: string[]; enum?: Json[]; default?: Json; minimum?: number; maximum?: number; [key: string]: unknown };
export interface PluginCommand { id: string; title: string; description: string; menus: ("global" | "component")[]; input_schema: JsonSchema; output_schema?: JsonSchema | null }
export interface PluginPanel { id: string; title: string; entry: string }
export interface PluginManifest {
  $schema?: string;
  schema_version: number; api_version: number; id: string; name: string; version: string; description: string; author: string;
  engines: { mcdh: string }; entry: string; commands: PluginCommand[]; panels: PluginPanel[];
  mcp_tools: { name: string; description: string; command: string }[]; settings_schema: JsonSchema;
}
export interface PluginSource { kind: "release" | "branch" | "local" | "local_zip" | "local_directory"; reference: string; archive_sha256: string }
export interface PluginSummary {
  manifest: PluginManifest; repository: string; source: PluginSource; fingerprint: string;
  enabled: boolean; trusted: boolean; compatible: boolean; allow_mcp: boolean; settings: Json;
  last_error?: { code: string; message: string } | null;
}
export interface PluginCandidate {
  token: string; manifest: PluginManifest; repository: string; source: PluginSource; fingerprint: string; compatible: boolean; already_installed: boolean;
}
export interface PluginProgress { phase: string; downloaded_bytes: number; total_bytes?: number | null }
export interface PluginPanelRequest { pluginId: string; panelId: string; fingerprint: string; componentId?: string | null }
export interface PluginPanelInfo { plugin: PluginSummary; panel: PluginPanel; environment: PluginEnvironment }
export interface PluginAction { panel_id: string; placement: "main" | "window"; component_id?: string | null }
export interface PluginExecution { value: Json; actions: PluginAction[] }
export interface PluginLog { time: string; level: string; text: string }
