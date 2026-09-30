import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LoaderCircle, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, errorMessage } from "./api";
import { applyAppearance } from "./appearance";
import { ComponentDialog, CreateDialog, DEFAULT_SETTINGS, ImportDialog, Modal, readIgnoredWarningKeys, SettingsDialog, StartupUpdateDialog, warningKey, WarningsDialog, writeIgnoredWarningKeys } from "./App";
import { UpdateProgress, useAppUpdate } from "./AppUpdate";
import { useMcdk } from "./Mcdk";
import { WindowChrome } from "./WindowChrome";
import { closeDialogWindow, confirmAction, nativeWindows, notifySettings, notifyWorkspace, openDialogWindow, SETTINGS_CHANGED, WORKSPACE_CHANGED, type DialogRequest } from "./windows";
import type { AppSettings, ComponentSummary, DiscoveryWarning, OperationResult, SourceRecord } from "./types";

const titles = { settings: "设置", create: "新建组件", import: "导入组件", warnings: "扫描问题", component: "组件配置", startup: "版本更新", confirm: "确认操作" };

export function DialogApp({ request }: { request: DialogRequest }) {
  const initialComponent = request.kind === "component" && request.initialComponent?.id === request.componentId ? request.initialComponent : null;
  const [settings, setSettings] = useState(request.kind === "component" ? request.initialSettings ?? DEFAULT_SETTINGS : DEFAULT_SETTINGS);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [warnings, setWarnings] = useState<DiscoveryWarning[]>([]);
  const [component, setComponent] = useState<ComponentSummary | null>(initialComponent);
  const [ignored, setIgnored] = useState(readIgnoredWarningKeys);
  const [loading, setLoading] = useState(!initialComponent);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const mcdk = useMcdk(setNotice);
  const updater = useAppUpdate();
  const loadRevision = useRef(0);
  const load = useCallback(async () => {
    const revision = ++loadRevision.current;
    const current = () => revision === loadRevision.current;
    setError("");
    try {
      await Promise.all([
        api.settings().then((value) => { if (current()) setSettings(value); }),
        request.kind === "component" ? api.component(request.componentId).then((value) => { if (current()) setComponent(value); }) :
        request.kind === "create" ? api.sources().then((value) => { if (current()) setSources(value); }) :
        request.kind === "warnings" ? api.refresh().then((result) => {
          if (current()) { setSources(result.sources); setWarnings(result.warnings); setIgnored(readIgnoredWarningKeys()); }
        }) : Promise.resolve(),
      ]);
    } catch (cause) { if (current()) setError(errorMessage(cause)); }
    finally { if (current()) setLoading(false); }
  }, [request]);

  useEffect(() => { void load(); return () => { loadRevision.current += 1; }; }, [load]);
  useLayoutEffect(() => {
    applyAppearance(settings);
  }, [settings.theme, settings.color_preset]);
  useEffect(() => {
    if (!nativeWindows) return;
    let active = true;
    void getCurrentWindow().show().catch((cause) => setError(errorMessage(cause)));
    const changed = listen(SETTINGS_CHANGED, () => {
      void api.settings().then((value) => { if (active) setSettings(value); }).catch((cause) => { if (active) setNotice(errorMessage(cause)); });
    });
    const workspace = listen(WORKSPACE_CHANGED, () => {
      if (request.kind === "create" || request.kind === "warnings") void load();
    });
    return () => { active = false; void changed.then((stop) => stop()); void workspace.then((stop) => stop()); };
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
