// Shared by the embedded backend and desktop panel host. No Node.js globals.
export function createContext(bridge, metadata, onLog) {
  const call = async (method, args = {}) => {
    const result = await bridge(method, args);
    if (!result.ok) throw Object.assign(new Error(result.error.message), { code: result.error.code });
    return result.value;
  };
  const commandHandlers = new Map();
  const disposables = [];
  const log = (level, ...values) => onLog(level, values.map((value) => {
    try { return typeof value === "string" ? value : JSON.stringify(value); }
    catch { return String(value); }
  }).join(" "));
  const context = {
    environment: metadata.environment,
    commands: {
      register(id, handler) {
        if (!metadata.commands.some((command) => command.id === id) || commandHandlers.has(id) || typeof handler !== "function") {
          throw new Error(`Invalid or duplicate command handler: ${id}`);
        }
        commandHandlers.set(id, handler);
        const dispose = () => commandHandlers.delete(id);
        disposables.push(dispose);
        return dispose;
      },
      invoke: (command, input = {}) => call("commands.invoke", { command, input }),
    },
    components: {
      list: () => call("components.list"),
      get: (componentId) => call("components.get", { component_id: componentId }),
      create: (request) => call("components.create", request),
      import: (request) => call("components.import", request),
      copy: (request) => call("components.copy", request),
      move: (request) => call("components.move", request),
      export: (request) => call("components.export", request),
      setMetadata: (request) => call("components.metadata", request),
      regenerateUuids: (componentId) => call("components.uuids", { component_id: componentId }),
      bumpVersion: (request) => call("components.version", request),
    },
    fs: {
      readText: (path) => call("fs.read_text", { path }),
      writeText: (path, text) => call("fs.write_text", { path, text }),
      readDirectory: (path) => call("fs.read_directory", { path }),
      stat: (path) => call("fs.stat", { path }),
      mkdir: (path) => call("fs.mkdir", { path }),
      remove: (path) => call("fs.remove", { path }),
    },
    http: { request: (request) => call("http.request", request) },
    process: { run: (request) => call("process.run", request) },
    settings: {
      get: () => call("settings.get"),
      set: (settings) => call("settings.set", { settings }),
    },
    storage: {
      get: (key) => call("storage.get", { key }),
      set: (key, value) => call("storage.set", { key, value }),
      delete: (key) => call("storage.delete", { key }),
    },
    ui: { openPanel: (panelId, placement = "main", componentId = metadata.environment.componentId) =>
      call("ui.open_panel", { panel_id: panelId, placement, component_id: componentId }) },
    log: {
      info: (...values) => log("info", ...values),
      warn: (...values) => log("warn", ...values),
      error: (...values) => log("error", ...values),
    },
    subscriptions: disposables,
  };
  return { context, handlers: commandHandlers, dispose() {
    let failure;
    for (const cleanup of disposables.reverse()) {
      try { cleanup(); } catch (error) { failure ??= error; }
    }
    disposables.length = 0;
    if (failure) throw failure;
  } };
}
