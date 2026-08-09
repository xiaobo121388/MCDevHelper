import { invoke } from "@tauri-apps/api/core";
import type {
  ComponentKind,
  ComponentSummary,
  DiscoveryResult,
  IdentityPolicy,
  OperationResult,
  SourceRecord,
  VersionPart,
} from "./types";

export const desktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  refresh: () => invoke<DiscoveryResult>("refresh_components"),
  sources: () => invoke<SourceRecord[]>("list_sources"),
  addSingle: (path: string) => invoke<SourceRecord>("add_single_component", { path }),
  addLibrary: (path: string) => invoke<SourceRecord>("add_library", { path }),
  removeSource: (sourceId: string) => invoke<boolean>("remove_source", { sourceId }),
  create: (request: {
    name: string;
    kind: ComponentKind;
    destination: string;
    mcs_compatible: boolean;
  }) => invoke<OperationResult>("create_component", { request }),
  import: (request: {
    source: string;
    destination: string;
    mcs_compatible: boolean;
    identity_policy: IdentityPolicy;
  }) => invoke<OperationResult>("import_component", { request }),
  copy: (request: {
    component_id: string;
    destination: string;
    mcs_compatible: boolean;
    identity_policy: Exclude<IdentityPolicy, "error">;
  }) => invoke<OperationResult>("copy_component", { request }),
  move: (request: { component_id: string; destination: string; mcs_compatible: boolean }) =>
    invoke<OperationResult>("move_component", { request }),
  export: (request: { component_id: string; destination: string }) =>
    invoke<OperationResult>("export_component", { request }),
  tags: (componentId: string, tags: string[]) =>
    invoke<OperationResult>("set_component_tags", {
      request: { component_id: componentId, tags },
    }),
  regenerateUuids: (componentId: string) =>
    invoke<OperationResult>("regenerate_manifest_uuids", { componentId }),
  bumpVersion: (componentId: string, part: VersionPart) =>
    invoke<OperationResult>("bump_manifest_version", {
      request: { component_id: componentId, part },
    }),
  openDirectory: (componentId: string) => invoke<void>("open_component_directory", { componentId }),
  openVsCode: (componentId: string) => invoke<void>("open_component_in_vscode", { componentId }),
  vscodeStatus: () =>
    invoke<{ available: boolean; path?: string; custom: boolean }>("vscode_status"),
  setVsCodePath: (path?: string) => invoke<void>("set_vscode_path", { path: path ?? null }),
  mcpClientConfig: () => invoke<string>("mcp_client_config"),
  component: (componentId: string) =>
    invoke<ComponentSummary>("get_component", { componentId }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return "操作失败，请检查路径和组件内容。";
}
