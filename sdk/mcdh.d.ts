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
export interface OperationResult { component?: Component; actual_path: string; modified_files: string[]; warnings: string[] }
export type CommandHandler = (input: Json, ctx: PluginContext) => Json | void | Promise<Json | void>;
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
    create(request: Json): Promise<OperationResult>;
    import(request: Json): Promise<OperationResult>;
    copy(request: Json): Promise<OperationResult>;
    move(request: Json): Promise<OperationResult>;
    export(request: Json): Promise<OperationResult>;
    setMetadata(request: Json): Promise<OperationResult>;
    regenerateUuids(componentId: string): Promise<OperationResult>;
    bumpVersion(request: Json): Promise<OperationResult>;
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
  onThemeChanged(listener: (theme: PanelContext["theme"]) => void): () => void;
  assetUrl(relativePath: string): string;
}
export interface PanelModule {
  mount(container: HTMLElement, context: PanelContext): void | (() => void) | Promise<void | (() => void)>;
}
