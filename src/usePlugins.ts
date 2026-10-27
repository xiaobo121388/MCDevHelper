import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "./api";
import { pluginApi, pluginDesktop } from "./pluginApi";
import { confirmAction, openDialogWindow, notifyWorkspace } from "./windows";
import type { Json, PluginCommand, PluginExecution, PluginPanelRequest, PluginProgress, PluginSummary } from "./pluginTypes";

export const PLUGIN_TRUST_WARNING = "插件拥有当前 Windows 用户权限，可以读写或删除本机文件、访问网络和运行程序。仅启用可信来源的插件。";
export const MCP_TRUST_WARNING = "允许 MCP 调用此版本插件？AI 将能够调用该插件提供的工具；插件可以读写或删除文件、联网和运行程序，不受内置 MCP 工具的功能限制。更新后需要重新授权。";

export function usePlugins(onNotice: (message: string) => void, onPanel: (request: PluginPanelRequest) => void) {
  const [items, setItems] = useState<PluginSummary[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<{ id: string; title: string } | null>(null);
  const [progress, setProgress] = useState<PluginProgress | null>(null);
  const [result, setResult] = useState<{ title: string; value: Json } | null>(null);
  const [prompt, setPrompt] = useState<{ plugin: PluginSummary; command: PluginCommand; componentId?: string } | null>(null);
  const alive = useRef(true);
  const active = useRef<string | null>(null);
  const reload = useCallback(async () => {
    if (!pluginDesktop) return;
    const next = await pluginApi.list();
    if (alive.current) setItems((current) => JSON.stringify(current) === JSON.stringify(next) ? current : next);
  }, []);
  useEffect(() => {
    alive.current = true;
    if (!pluginDesktop) return;
    void reload().catch((cause) => { if (alive.current) setError(errorMessage(cause)); });
    const timer = window.setInterval(() => { void reload().catch((cause) => { if (alive.current) setError(errorMessage(cause)); }); }, 2000);
    return () => { alive.current = false; window.clearInterval(timer); if (active.current) void pluginApi.cancel(active.current).catch(() => {}); };
  }, [reload]);

  const operation = useCallback(async <T,>(title: string, work: (id: string) => Promise<T>): Promise<T> => {
    if (active.current) throw new Error("已有插件任务正在执行。");
    const id = crypto.randomUUID();
    active.current = id;
    setBusy({ id, title }); setError(""); setProgress(null);
    try { const value = await work(id); await reload(); return value; }
    catch (cause) { if (alive.current) setError(errorMessage(cause)); throw cause; }
    finally { active.current = null; if (alive.current) { setBusy(null); setProgress(null); } }
  }, [reload]);

  const applyActions = useCallback(async (plugin: Pick<PluginSummary, "manifest" | "fingerprint">, execution: PluginExecution) => {
    for (const action of execution.actions) {
      const request: PluginPanelRequest = { pluginId: plugin.manifest.id, fingerprint: plugin.fingerprint, panelId: action.panel_id, componentId: action.component_id };
      if (action.placement === "window") await openDialogWindow({ kind: "plugin_panel", ...request });
      else onPanel(request);
    }
  }, [onPanel]);

  const execute = useCallback(async (plugin: PluginSummary, command: PluginCommand, input: Json, componentId?: string) => {
    setPrompt(null);
    try {
      const execution = await operation(command.title, (id) => pluginApi.execute(plugin.manifest.id, plugin.fingerprint, command.id, input, componentId, id));
      await applyActions(plugin, execution);
      await notifyWorkspace();
      if (!execution.actions.length) setResult({ title: command.title, value: execution.value });
      onNotice(`${command.title}已完成。`);
    } catch { /* The persistent task error is rendered by the host. */ }
  }, [operation, applyActions, onNotice]);

  const requestCommand = (plugin: PluginSummary, command: PluginCommand, componentId?: string) => {
    if (Object.keys(command.input_schema.properties ?? {}).length || command.input_schema.required?.length) setPrompt({ plugin, command, componentId });
    else void execute(plugin, command, {}, componentId);
  };
  const configure = async (plugin: PluginSummary, enabled: boolean, allowMcp: boolean, settings = plugin.settings) => {
    if (allowMcp && !plugin.allow_mcp && !await confirmAction(`${MCP_TRUST_WARNING}\n\n${plugin.manifest.name} ${plugin.manifest.version}\n${plugin.repository}`)) return;
    await operation("保存插件设置", () => pluginApi.configure(plugin, enabled, allowMcp, settings));
  };
  return { items, error, setError, busy, progress, result, setResult, prompt, setPrompt, reload, operation, applyActions, execute, requestCommand, configure,
    preview: (source: string) => operation("获取插件", (id) => pluginApi.preview(source, id, setProgress)),
    cancel: () => active.current ? pluginApi.cancel(active.current) : Promise.resolve(),
    openPanel: onPanel,
  };
}
export type PluginController = ReturnType<typeof usePlugins>;
