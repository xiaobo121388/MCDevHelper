import { Channel, convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import type { Json, PluginCandidate, PluginExecution, PluginLog, PluginPanelInfo, PluginPanelRequest, PluginProgress, PluginSummary } from "./pluginTypes";

export const pluginDesktop = isTauri();
export const pluginApi = {
  list: () => invoke<PluginSummary[]>("list_plugins"),
  preview(source: string, operationId: string, progress: (value: PluginProgress) => void) {
    const onProgress = new Channel<PluginProgress>();
    onProgress.onmessage = progress;
    return invoke<PluginCandidate>("preview_plugin", { source, operationId, onProgress });
  },
  install: (token: string, fingerprint: string) => invoke<PluginSummary>("install_plugin", { token, fingerprint }),
  previewLocal: (path: string, operationId: string) => invoke<PluginCandidate>("preview_local_plugin", { path, operationId }),
  discard: (token: string) => invoke<void>("discard_plugin_preview", { token }),
  configure: (plugin: PluginSummary, enabled: boolean, allowMcp: boolean, settings: Json) =>
    invoke<PluginSummary>("configure_plugin", { pluginId: plugin.manifest.id, fingerprint: plugin.fingerprint, enabled, allowMcp, settings }),
  uninstall: (pluginId: string, clearData: boolean) => invoke<void>("uninstall_plugin", { pluginId, clearData }),
  execute: (pluginId: string, fingerprint: string, command: string, input: Json, componentId: string | null | undefined, operationId: string) =>
    invoke<PluginExecution>("execute_plugin_command", { request: { plugin_id: pluginId, fingerprint, command, input, component_id: componentId ?? null }, operationId }),
  cancel: (operationId: string) => invoke<void>("cancel_plugin_operation", { operationId }),
  panel: (request: PluginPanelRequest) => invoke<PluginPanelInfo>("get_plugin_panel", request as unknown as Record<string, unknown>),
  panelCall: (request: PluginPanelRequest, method: string, args: Json, operationId: string) =>
    invoke<PluginExecution>("plugin_panel_call", { pluginId: request.pluginId, fingerprint: request.fingerprint, method, args, operationId }),
  logs: (pluginId: string) => invoke<PluginLog[]>("get_plugin_logs", { pluginId }),
};

export function pluginAssetUrl(request: PluginPanelRequest, path: string) {
  return convertFileSrc(`${request.pluginId}/${request.fingerprint}/${path}`, "mcdh-plugin");
}
