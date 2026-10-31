export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type Placement = "main" | "window";
export interface PluginEnvironment {
  caller: "desktop" | "mcp";
  pluginId: string;
  appVersion: string;
  packageDir: string;
  dataDir: string;
  componentId?: string | null;
}
export interface Component {
  id: string;
  name: string;
  kind: "addon" | "material" | "map";
  path: string;
  tags: string[];
  favorite: boolean;
  [key: string]: unknown;
}
export interface OperationResult { component: Component | null; actual_path: string; modified_files: string[]; warnings: string[] }
export type ComponentKind = Component["kind"];
export type IdentityPolicy = "preserve" | "regenerate" | "error";
export interface CreateComponentRequest { name: string; kind: ComponentKind; destination: string; mcs_compatible: boolean; namespace?: string }
export interface ImportComponentRequest { source: string; destination: string; mcs_compatible: boolean; identity_policy: IdentityPolicy; content_mode?: "clean" | "full" }
export interface CopyComponentRequest { component_id: string; destination: string; mcs_compatible: boolean; identity_policy: IdentityPolicy }
export interface MoveComponentRequest { component_id: string; destination: string; mcs_compatible: boolean }
export interface ExportComponentRequest { component_id: string; destination: string; content_mode?: "clean" | "full"; conflict_policy?: "rename" | "overwrite" | "error" }
export interface ComponentMetadataRequest { component_id: string; display_name: string; tags: string[]; favorite: boolean }
export interface BumpVersionRequest { component_id: string; part: "major" | "minor" | "patch" }
export type CommandHandler = (input: { [key: string]: Json }, ctx: PluginContext) => Json | void | Promise<Json | void>;
export interface PluginContext {
  environment: PluginEnvironment;
  commands: {
    /** Backend only; handlers are registered for each invocation. */
    register(id: string, handler: CommandHandler): () => void;
    /** Panels only; invokes a backend command belonging to this plugin. */
    invoke(command: string, input?: Json): Promise<Json>;
  };
  components: {
    list(): Promise<Component[]>;
    get(componentId: string): Promise<Component>;
    create(request: CreateComponentRequest): Promise<OperationResult>;
    import(request: ImportComponentRequest): Promise<OperationResult>;
    copy(request: CopyComponentRequest): Promise<OperationResult>;
    move(request: MoveComponentRequest): Promise<OperationResult>;
    export(request: ExportComponentRequest): Promise<OperationResult>;
    setMetadata(request: ComponentMetadataRequest): Promise<OperationResult>;
    regenerateUuids(componentId: string): Promise<OperationResult>;
    bumpVersion(request: BumpVersionRequest): Promise<OperationResult>;
  };
  fs: {
    readText(path: string): Promise<string>;
    writeText(path: string, text: string): Promise<void>;
    readDirectory(path: string): Promise<{ name: string; directory: boolean }[]>;
    stat(path: string): Promise<{ directory: boolean; size: number }>;
    mkdir(path: string): Promise<void>;
    /** Removes a file or an empty directory, never recursively. */
    remove(path: string): Promise<void>;
  };
  http: { request(request: { url: string; method?: string; headers?: Record<string, string>; body?: string }): Promise<{ status: number; headers: Record<string, string>; body: string }> };
  process: { run(request: { program: string; args?: string[]; cwd?: string; env?: Record<string, string>; timeout_ms?: number }): Promise<{ exit_code: number; stdout: string; stderr: string; truncated: boolean }> };
  settings: { get(): Promise<Json>; set(settings: Json): Promise<Json> };
  storage: { get(key: string): Promise<Json>; set(key: string, value: Json): Promise<void>; delete(key: string): Promise<void> };
  ui: { openPanel(panelId: string, placement?: Placement, componentId?: string | null): Promise<void> };
  log: { info(...values: unknown[]): void; warn(...values: unknown[]): void; error(...values: unknown[]): void };
  subscriptions: (() => void)[];
}
export interface PanelContext extends PluginContext {
  theme: { mode: "light" | "dark"; accent: string };
  /** Optional on older API 1 hosts. Describes this panel instance, not global state. */
  readonly placement?: Placement;
  /** Desktop panel only. Loading/errors always retain the host recovery controls. */
  setHeaderVisible?(visible: boolean): void;
  onThemeChanged(listener: (theme: PanelContext["theme"]) => void): () => void;
  assetUrl(relativePath: string): string;
}
export interface PanelModule {
  mount(container: HTMLElement, context: PanelContext): void | (() => void) | Promise<void | (() => void)>;
}
