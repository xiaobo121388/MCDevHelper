import { useEffect, useRef, useState, type FormEvent } from "react";
import { Archive, Check, ChevronDown, Download, ExternalLink, FileText, FolderOpen, LoaderCircle, Plus, Puzzle, RefreshCw, Save, ShieldAlert, Trash2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { openDialogWindow } from "./windows";
import { errorMessage } from "./api";
import { pluginApi } from "./pluginApi";
import { SchemaFields } from "./PluginForms";
import { PLUGIN_TRUST_WARNING, usePlugins, type PluginController } from "./usePlugins";
import type { Json, PluginCandidate, PluginLog, PluginPanelRequest, PluginSummary } from "./pluginTypes";

export function PluginSettings({ onNotice, onOpenPanel }: { onNotice: (message: string) => void; onOpenPanel: (request: PluginPanelRequest) => void }) {
  const controller = usePlugins(onNotice, onOpenPanel);
  return <PluginManager controller={controller} />;
}

export function PluginManager({ controller }: { controller: PluginController }) {
  const [source, setSource] = useState("");
  const sourceInput = useRef<HTMLInputElement>(null);
  const [candidate, setCandidate] = useState<PluginCandidate | null>(null);
  const [trusted, setTrusted] = useState(false);
  const currentPreview = useRef<string | null>(null);
  const mounted = useRef(true);
  const choosing = useRef(false);
  const [selecting, setSelecting] = useState(false);
  useEffect(() => { mounted.current = true; return () => {
    mounted.current = false;
    if (currentPreview.current) void pluginApi.discard(currentPreview.current).catch(() => {});
  }; }, []);
  const discard = async () => {
    if (currentPreview.current) await pluginApi.discard(currentPreview.current);
    currentPreview.current = null; setCandidate(null); setTrusted(false);
  };
  const preview = async (repository: string, expectedId?: string, local = false) => {
    try {
      await discard();
      const next = await (local ? controller.previewLocal(repository) : controller.preview(repository));
      if (!mounted.current) { await pluginApi.discard(next.token); return; }
      if (expectedId && next.manifest.id !== expectedId) { await pluginApi.discard(next.token); throw new Error("所选来源的插件 ID 已变化，无法作为原插件更新。"); }
      currentPreview.current = next.token; setCandidate(next); setTrusted(false);
    } catch (cause) { if (mounted.current) controller.setError(errorMessage(cause)); }
  };
  const importLocal = async (directory: boolean, expectedId?: string) => {
    if (choosing.current || controller.busy) return;
    choosing.current = true; setSelecting(true);
    try {
      const selected = await open(directory
        ? { title:"导入插件目录", directory:true, multiple:false }
        : { title:"导入插件 ZIP", multiple:false, filters:[{name:"MCDH 插件包",extensions:["zip"]}] });
      if (typeof selected === "string" && mounted.current) await preview(selected, expectedId, true);
    } catch (cause) { if (mounted.current) controller.setError(errorMessage(cause)); }
    finally { choosing.current = false; if (mounted.current) setSelecting(false); }
  };
  const install = async () => {
    if (!candidate || !trusted) return;
    try {
      await controller.operation("安装插件", () => pluginApi.install(candidate.token, candidate.fingerprint));
      currentPreview.current = null; setCandidate(null); setTrusted(false);
    } catch { /* Persistent error belongs to the controller. */ }
  };
  const submit = (event: FormEvent) => { event.preventDefault(); void preview(source); };
  return <section className="plugin-manager" aria-label="插件管理">
    <header className="plugin-manager-header"><h3>插件</h3><div className="plugin-manager-tools"><button className="icon-button" title="刷新插件" aria-label="刷新插件" disabled={!!controller.busy} onClick={() => void controller.reload().catch((cause) => controller.setError(errorMessage(cause)))}><RefreshCw size={18} /></button></div></header>
    <div className="plugin-manager-body">
      <form className="plugin-install-bar" onSubmit={submit}><label><Puzzle size={18} /><input ref={sourceInput} aria-label="GitHub 插件仓库" value={source} onChange={(event) => setSource(event.target.value)} placeholder="owner/repo" required disabled={!!controller.busy} />{source && <button className="input-clear" type="button" title="清空插件仓库" aria-label="清空插件仓库" disabled={!!controller.busy} onClick={() => { setSource(""); sourceInput.current?.focus(); }}><X size={16} /></button>}</label><button className="button primary" type="submit" aria-busy={!!controller.busy} disabled={!!controller.busy || selecting || !source.trim()}><Plus size={16} />添加插件</button><div className="plugin-local-import"><button className="button secondary" type="button" title="导入插件 ZIP" disabled={!!controller.busy || selecting} onClick={() => void importLocal(false)}><Archive size={16} />本地导入</button><button className="icon-button" type="button" title="导入插件目录" aria-label="导入插件目录" disabled={!!controller.busy || selecting} onClick={() => void importLocal(true)}><FolderOpen size={17} /></button></div></form>
      {controller.busy && <div className="plugin-task" role="status"><LoaderCircle className="spin" size={17} /><span>{controller.busy.title}{controller.progress?.phase === "downloading" ? ` · ${(controller.progress.downloaded_bytes / 1048576).toFixed(1)} MB` : ""}</span><button className="icon-button" title="取消插件任务" aria-label="取消插件任务" onClick={() => void controller.cancel().catch((cause) => controller.setError(errorMessage(cause)))}><X size={16} /></button></div>}
      {candidate && <section className="plugin-preview" aria-label="插件安装预览">
        <header><div><h2>{candidate.manifest.name}<span>v{candidate.manifest.version}</span></h2><p>{candidate.manifest.description}</p></div><button className="icon-button" title="关闭安装预览" aria-label="关闭安装预览" disabled={!!controller.busy} onClick={() => void discard().catch((cause) => controller.setError(errorMessage(cause)))}><X size={17} /></button></header>
        <dl className="plugin-metadata"><dt>来源</dt><dd>{candidate.repository}</dd><dt>作者声明</dt><dd>{candidate.manifest.author || "未提供"}</dd><dt>版本来源</dt><dd>{candidate.source.kind === "release" ? "Release" : candidate.source.kind === "branch" ? "默认分支提交" : "本地包"} · {candidate.source.reference}</dd><dt>兼容范围</dt><dd>MCDH {candidate.manifest.engines.mcdh}</dd><dt>内容指纹</dt><dd className="plugin-fingerprint" title={candidate.fingerprint}>{candidate.fingerprint}</dd></dl>
        {!candidate.compatible && <p className="plugin-error" role="alert">此插件不兼容当前 MCDH 版本，无法安装。</p>}
        {candidate.already_installed && <p className="plugin-state"><Check size={15} />已安装相同内容</p>}
        <div className="plugin-trust-warning"><ShieldAlert size={20} /><p>{PLUGIN_TRUST_WARNING}</p></div>
        <label className="plugin-checkbox"><input type="checkbox" checked={trusted} disabled={!!controller.busy} onChange={(event) => setTrusted(event.target.checked)} /><span>我信任此插件，并允许它以当前用户权限运行</span></label>
        <div className="dialog-actions"><button className="button secondary" disabled={!!controller.busy} onClick={() => void discard()}>取消</button><button className="button primary" disabled={!!controller.busy || !trusted || !candidate.compatible} onClick={() => void install()}><Download size={16} />确认安装并启用</button></div>
      </section>}
      {controller.error && <div className="plugin-error" role="alert"><ShieldAlert size={17} /><span>{controller.error}</span><button className="icon-button" aria-label="关闭插件错误" title="关闭插件错误" onClick={() => controller.setError("")}><X size={15} /></button></div>}
      <div className="plugin-list-heading"><h2>已安装</h2><span>{controller.items.length}</span></div>
      <div className="plugin-list">{controller.items.map((plugin) => <PluginRow key={plugin.manifest.id} plugin={plugin} controller={controller} onUpdate={() => void (plugin.source.kind.startsWith("local") ? importLocal(plugin.source.kind === "local_directory", plugin.manifest.id) : preview(plugin.repository, plugin.manifest.id))} />)}</div>
      {!controller.items.length && <div className="plugin-empty"><Puzzle size={30} /><h2>暂无插件</h2></div>}
    </div>
  </section>;
}

function PluginRow({ plugin, controller, onUpdate }: { plugin: PluginSummary; controller: PluginController; onUpdate: () => void }) {
  const [logs, setLogs] = useState<PluginLog[] | null>(null);
  const [removing, setRemoving] = useState(false);
  const [clearData, setClearData] = useState(false);
  const [settings, setSettings] = useState<Record<string, Json>>((plugin.settings ?? {}) as Record<string, Json>);
  const editingSettings = useRef(false);
  useEffect(() => { if (!editingSettings.current) setSettings((plugin.settings ?? {}) as Record<string, Json>); }, [plugin.settings]);
  const attempt = async (work: () => Promise<unknown>) => { try { await work(); } catch (cause) { controller.setError(errorMessage(cause)); } };
  const status = !plugin.compatible ? "不兼容" : !plugin.trusted ? "信任失效" : plugin.enabled ? "已启用" : "已禁用";
  return <article className="plugin-row">
    <div className="plugin-row-top"><span className="plugin-row-icon"><Puzzle size={22} /></span><div className="plugin-row-name"><h3>{plugin.manifest.name}<small>v{plugin.manifest.version}</small></h3><p>{plugin.manifest.description || plugin.repository}</p><div className="plugin-row-origin">{plugin.repository}<span className={plugin.enabled && plugin.trusted ? "plugin-state enabled" : "plugin-state"}>{status}</span></div></div><label className="plugin-enable" title={`启用 ${plugin.manifest.name}`}><input type="checkbox" role="switch" aria-label={`启用 ${plugin.manifest.name}`} checked={plugin.enabled} disabled={!!controller.busy || !plugin.trusted || !plugin.compatible} onChange={(event) => void attempt(() => controller.configure(plugin, event.target.checked, event.target.checked && plugin.allow_mcp))} /></label></div>
    {plugin.last_error && <p className="plugin-row-error">{plugin.last_error.message}</p>}
    <div className="plugin-row-actions">
      <label className="plugin-checkbox"><input type="checkbox" checked={plugin.allow_mcp} aria-label={`允许 MCP 调用 ${plugin.manifest.name}`} disabled={!!controller.busy || !plugin.enabled || !plugin.trusted || !plugin.manifest.mcp_tools.length} onChange={(event) => void attempt(() => controller.configure(plugin, plugin.enabled, event.target.checked))} /><span>允许 MCP 调用</span></label>
      <span className="plugin-action-spacer" />
      {plugin.manifest.panels.map((panel) => <span className="plugin-panel-actions" key={panel.id}><button className="button secondary" disabled={!plugin.enabled || !plugin.trusted} onClick={() => controller.openPanel({ pluginId:plugin.manifest.id, fingerprint:plugin.fingerprint, panelId:panel.id })}>{panel.title}</button><button className="icon-button" disabled={!plugin.enabled || !plugin.trusted} title={`独立窗口打开 ${panel.title}`} aria-label={`独立窗口打开 ${panel.title}`} onClick={() => void attempt(() => openDialogWindow({ kind:"plugin_panel", pluginId:plugin.manifest.id, fingerprint:plugin.fingerprint, panelId:panel.id }))}><ExternalLink size={15} /></button></span>)}
      <button className="icon-button" disabled={!!controller.busy} title={`检查更新 ${plugin.manifest.name}`} aria-label={`检查更新 ${plugin.manifest.name}`} onClick={onUpdate}><RefreshCw size={16} /></button>
      <button className="icon-button" title={`日志 ${plugin.manifest.name}`} aria-label={`日志 ${plugin.manifest.name}`} aria-expanded={!!logs} onClick={() => void attempt(async () => setLogs(logs ? null : await pluginApi.logs(plugin.manifest.id)))}><FileText size={16} /></button>
      <button className="icon-button plugin-uninstall" disabled={!!controller.busy} title={`卸载 ${plugin.manifest.name}`} aria-label={`卸载 ${plugin.manifest.name}`} onClick={() => setRemoving(!removing)}><Trash2 size={16} /></button>
    </div>
    {!!Object.keys(plugin.manifest.settings_schema.properties ?? {}).length && <details className="plugin-settings"><summary>设置<ChevronDown size={15} /></summary><form onSubmit={(event) => {
      event.preventDefault();
      void attempt(async () => { await controller.configure(plugin, plugin.enabled, plugin.allow_mcp, settings); editingSettings.current = false; });
    }}><SchemaFields schema={plugin.manifest.settings_schema} value={settings} onChange={(value) => { editingSettings.current = true; setSettings(value); }} disabled={!!controller.busy} /><button className="button secondary" type="submit" disabled={!!controller.busy}><Save size={15} />保存</button></form></details>}
    {logs && <div className="plugin-logs" role="log">{logs.length ? logs.map((log, index) => <div key={index}><time>{log.time}</time><strong>{log.level}</strong><span>{log.text}</span></div>) : <p>暂无运行日志</p>}</div>}
    {removing && <div className="plugin-remove-confirm"><p>卸载 {plugin.manifest.name}？</p><label className="plugin-checkbox"><input type="checkbox" checked={clearData} onChange={(event) => setClearData(event.target.checked)} /><span>同时清除插件设置、存储和日志</span></label><div className="dialog-actions"><button className="button secondary" onClick={() => setRemoving(false)}>取消</button><button className="button danger" disabled={!!controller.busy} onClick={() => void attempt(() => controller.uninstall(plugin.manifest.id, clearData))}><Trash2 size={15} />确认卸载</button></div></div>}
  </article>;
}
