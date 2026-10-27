import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ExternalLink, LoaderCircle, RefreshCw, TriangleAlert } from "lucide-react";
import { createContext } from "../sdk/context.js";
import type { PanelContext } from "../sdk/mcdh";
import { errorMessage } from "./api";
import { pluginApi, pluginAssetUrl } from "./pluginApi";
import { loadPluginModule } from "./pluginModules";
import { openDialogWindow } from "./windows";
import type { Json, PluginExecution, PluginPanelRequest } from "./pluginTypes";

function currentTheme(): PanelContext["theme"] {
  const style = getComputedStyle(document.documentElement);
  return { mode: style.colorScheme === "dark" ? "dark" : "light", accent: style.getPropertyValue("--accent").trim() };
}

export function PluginPanelHost({ request, onOpenPanel, onClose, standalone = false }: {
  request: PluginPanelRequest; onOpenPanel: (request: PluginPanelRequest) => void; onClose: () => void; standalone?: boolean;
}) {
  const container = useRef<HTMLDivElement>(null);
  const [title, setTitle] = useState("插件工具");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const callbacks = useRef({ onOpenPanel });
  callbacks.current = { onOpenPanel };
  const key = JSON.stringify(request);
  useEffect(() => {
    let active = true;
    let stopped = false;
    let cleanup: (() => void) | undefined;
    let dispose: (() => void) | undefined;
    let observer: MutationObserver | undefined;
    const jobs = new Set<string>();
    const node = container.current!;
    const shadow = node.shadowRoot ?? node.attachShadow({ mode: "open" });
    shadow.replaceChildren();
    setLoading(true); setError("");
    const stop = () => {
      stopped = true;
      for (const id of jobs) void pluginApi.cancel(id).catch(() => {});
      try { cleanup?.(); } catch { /* Third-party cleanup must not prevent host cleanup. */ }
      cleanup = undefined;
      try { dispose?.(); } catch { /* All subscriptions are attempted by the SDK. */ }
      dispose = undefined;
      observer?.disconnect(); if (active) shadow.replaceChildren();
    };
    const actions = async (result: PluginExecution) => {
      if (!active || stopped) return;
      for (const action of result.actions) {
        const next = { ...request, panelId: action.panel_id, componentId: action.component_id };
        if (action.placement === "window") await openDialogWindow({ kind: "plugin_panel", ...next });
        else callbacks.current.onOpenPanel(next);
      }
    };
    const bridge = async (method: string, args: Json) => {
      const id = crypto.randomUUID();
      if (!active || stopped) return { ok: false as const, error: { code: "panel_closed", message: "插件面板已关闭。" } };
      jobs.add(id);
      try {
        const values = args as Record<string, Json>;
        const result = method === "commands.invoke" ? await pluginApi.execute(request.pluginId, request.fingerprint, String(values.command), values.input ?? {}, request.componentId, id) :
          await pluginApi.panelCall(request, method, args, id);
        await actions(result);
        return { ok: true as const, value: result.value };
      } catch (cause) {
        return { ok: false as const, error: { code: cause && typeof cause === "object" && "code" in cause ? String(cause.code) : "plugin_error", message: errorMessage(cause) } };
      } finally { jobs.delete(id); }
    };
    void (async () => {
      try {
        const info = await pluginApi.panel(request);
        if (!active || stopped) return;
        setTitle(info.panel.title);
        const state = createContext(bridge, { commands: info.plugin.manifest.commands, environment: info.environment }, (level, text) => {
          void bridge("log.write", { level, text }).catch((cause) => { if (active) setError(errorMessage(cause)); });
        });
        dispose = state.dispose;
        const themeListeners = new Set<(theme: PanelContext["theme"]) => void>();
        const context: PanelContext = { ...state.context, theme: currentTheme(),
          onThemeChanged(listener) { themeListeners.add(listener); const stop = () => { themeListeners.delete(listener); }; state.context.subscriptions.push(stop); return stop; },
          assetUrl: (path) => pluginAssetUrl(request, path),
        };
        observer = new MutationObserver(() => {
          context.theme = currentTheme();
          for (const listener of themeListeners) {
            try { listener(context.theme); } catch (cause) { setError(errorMessage(cause)); }
          }
        });
        observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "style"] });
        const style = document.createElement("style");
        style.textContent = ":host{display:block;height:100%;color:var(--text);font-family:inherit;letter-spacing:0}.plugin-root{min-height:100%;box-sizing:border-box}*{box-sizing:border-box}button,input,select,textarea{font:inherit;letter-spacing:0}";
        const root = document.createElement("div"); root.className = "plugin-root";
        shadow.append(style, root);
        const module = await loadPluginModule(pluginAssetUrl(request, info.panel.entry));
        if (!active || stopped) return;
        const mounted = await module.mount(root, context);
        if (!active || stopped) { try { if (typeof mounted === "function") mounted(); } finally { state.dispose(); } return; }
        cleanup = typeof mounted === "function" ? mounted : undefined;
        setLoading(false);
      } catch (cause) { stop(); if (active) { setError(errorMessage(cause)); setLoading(false); } }
    })();
    const timer = window.setInterval(() => {
      if (stopped) return;
      void pluginApi.list().then((plugins) => {
        const current = plugins.find((plugin) => plugin.manifest.id === request.pluginId);
        if (!current?.enabled || !current.trusted || !current.compatible || current.fingerprint !== request.fingerprint) throw new Error("插件已停用、更新或信任失效，请重新打开面板。");
      }).catch((cause) => {
        if (!active) return;
        setError(errorMessage(cause));
        stop();
      });
    }, 2000);
    return () => {
      active = false; window.clearInterval(timer); stop();
    };
  // The serialized request includes plugin version and component context.
  }, [key, revision]);
  return <main className={standalone ? "plugin-panel-workspace standalone" : "workspace plugin-panel-workspace"}>
    <header className="topbar"><div className="plugin-panel-title"><button className="icon-button" title="关闭插件面板" aria-label="关闭插件面板" onClick={onClose}><ArrowLeft size={18} /></button><h1>{title}</h1></div><div className="top-actions"><button className="icon-button" title="重新加载面板" aria-label="重新加载面板" onClick={() => setRevision((value) => value + 1)}><RefreshCw size={17} /></button>{!standalone && <button className="icon-button" title="在独立窗口打开" aria-label="在独立窗口打开" onClick={() => void openDialogWindow({ kind: "plugin_panel", ...request }).catch((cause) => setError(errorMessage(cause)))}><ExternalLink size={17} /></button>}</div></header>
    {loading && <div className="plugin-panel-status" role="status"><LoaderCircle className="spin" size={20} />正在加载插件</div>}
    {error && <div className="plugin-error" role="alert"><TriangleAlert size={18} /><span>{error}</span></div>}
    <div className="plugin-panel-mount" ref={container} hidden={loading || !!error} />
  </main>;
}
