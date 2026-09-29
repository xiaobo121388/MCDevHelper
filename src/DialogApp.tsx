import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LoaderCircle, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage } from "./api";
import { ComponentDialog, CreateDialog, DEFAULT_SETTINGS, ImportDialog, Modal, readIgnoredWarningKeys, SettingsDialog, StartupUpdateDialog, warningKey, WarningsDialog, writeIgnoredWarningKeys } from "./App";
import { UpdateProgress, useAppUpdate } from "./AppUpdate";
import { useMcdk } from "./Mcdk";
import { WindowChrome } from "./WindowChrome";
import { closeDialogWindow, confirmAction, nativeWindows, notifySettings, notifyWorkspace, openDialogWindow, SETTINGS_CHANGED, WORKSPACE_CHANGED, type DialogRequest } from "./windows";
import type { AppSettings, ComponentSummary, DiscoveryWarning, OperationResult, SourceRecord } from "./types";

const titles = { settings: "设置", create: "新建组件", import: "导入组件", warnings: "扫描问题", component: "组件配置", startup: "版本更新", confirm: "确认操作" };

export function DialogApp({ request }: { request: DialogRequest }) {
  const [settings, setSettings] = useState(DEFAULT_SETTINGS);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [warnings, setWarnings] = useState<DiscoveryWarning[]>([]);
  const [component, setComponent] = useState<ComponentSummary | null>(null);
  const [ignored, setIgnored] = useState(readIgnoredWarningKeys);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const mcdk = useMcdk(setNotice);
  const updater = useAppUpdate();
  const load = useCallback(async () => {
    setError("");
    try {
      setSettings(await api.settings());
      if (request.kind === "component") setComponent(await api.component(request.componentId));
      if (request.kind === "create") setSources(await api.sources());
      if (request.kind === "warnings") {
        const result = await api.refresh();
        setSources(result.sources); setWarnings(result.warnings); setIgnored(readIgnoredWarningKeys());
      }
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setLoading(false); }
  }, [request]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
  }, [settings.theme]);
  useEffect(() => {
    if (!nativeWindows) return;
    void getCurrentWindow().show().catch((cause) => setError(errorMessage(cause)));
    const changed = listen(SETTINGS_CHANGED, () => { void load(); });
    const workspace = listen(WORKSPACE_CHANGED, () => {
      if (request.kind === "create" || request.kind === "warnings") void load();
    });
    return () => { void changed.then((stop) => stop()); void workspace.then((stop) => stop()); };
  }, [load, request.kind]);
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(""), 4200);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const close = () => { void closeDialogWindow().catch((cause) => setNotice(errorMessage(cause))); };
  const changed = () => { void notifyWorkspace().catch((cause) => setNotice(errorMessage(cause))); };
  const done = (message: string, operation?: OperationResult, refreshAfter = true) => {
    void notifyWorkspace(message, operation, refreshAfter).then(close).catch((cause) => setNotice(errorMessage(cause)));
  };
  const onSettings = useCallback((next: AppSettings) => { setSettings(next); }, []);
  const removeSource = async (source: SourceRecord) => {
    if (!await confirmAction(`确定从 MCDH 中移除来源？\n${source.path}\n磁盘文件不会被删除。`)) return;
    await api.removeSource(source.id);
    if (settings.default_destination === source.path) {
      setSettings(await api.setSettings({ ...settings, default_destination: undefined }));
      await notifySettings();
    }
    await load();
    await notifyWorkspace("来源记录已移除，磁盘文件未删除。");
  };
  const answer = async (value: boolean) => {
    if (request.kind !== "confirm") return;
    try { await emitTo(request.owner, `mcdh:confirm-${request.token}`, value); close(); }
    catch (cause) { setNotice(errorMessage(cause)); }
  };

  return <div className="dialog-window">
    <WindowChrome title={titles[request.kind] + " · MCDH"} busy={!!updater.progress} onError={setNotice} />
    {updater.progress ? <UpdateProgress progress={updater.progress} standalone /> : loading ? <div className="window-loading" role="status"><LoaderCircle className="spin" size={22} />正在加载</div> : error ?
      <div className="window-load-error" role="alert"><TriangleAlert size={24} /><p>{error}</p><button className="button secondary" onClick={() => { setLoading(true); void load(); }}>重试</button></div> : <>
        {request.kind === "settings" && <SettingsDialog settings={settings} mcdk={mcdk} updater={updater} onSettings={onSettings} onClose={close} onChanged={changed} onNotice={setNotice} />}
        {request.kind === "create" && <CreateDialog sources={sources} settings={settings} onConfigurePaths={() => { void openDialogWindow({ kind: "settings" }).catch((cause) => setNotice(errorMessage(cause))); }} onClose={close} onDone={done} />}
        {request.kind === "import" && <ImportDialog onClose={close} onDone={done} />}
        {request.kind === "component" && component && <ComponentDialog component={component} running={mcdk.status?.session?.component_id === component.id} onClose={close} onDone={done} onNotice={setNotice} />}
        {request.kind === "warnings" && <WarningsDialog warnings={warnings} sources={sources} ignoredKeys={ignored} onIgnore={(warning, value) => {
          const next = new Set(ignored);
          if (value) next.add(warningKey(warning)); else next.delete(warningKey(warning));
          setIgnored(next); writeIgnoredWarningKeys(next); changed();
        }} onRemoveSource={removeSource} onClose={close} onNotice={setNotice} />}
        {request.kind === "startup" && <StartupUpdateDialog dialog={request.dialog} updater={updater} onClose={close} />}
        {request.kind === "confirm" && <Modal title="确认操作" onClose={close}><div className="confirm-content"><TriangleAlert size={24} /><p>{request.message}</p></div><div className="dialog-actions"><button className="button secondary" autoFocus onClick={() => void answer(false)}>取消</button><button className="button primary" onClick={() => void answer(true)}>确认</button></div></Modal>}
      </>}
    {updater.error && <div className="app-update-error" role="alert"><strong>更新未完成</strong><p>{updater.error}</p><button className="button secondary" onClick={updater.clearError}>知道了</button></div>}
    {notice && <div className="toast" role="status">{notice}</div>}
  </div>;
}
