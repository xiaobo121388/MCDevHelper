import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, FolderOpen, LoaderCircle, Save, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage } from "./api";
import { nativeWindows, notifyWorkspace, WORKSPACE_CHANGED, type ExportDirectoryRequest, type WorkspaceChange } from "./windows";
import type { ExportSourceInfo } from "./types";

function DirectoryInput({ label, value, onChange, disabled, onError, autoFocus = false }: { label: string; value: string; onChange: (path: string) => void; disabled: boolean; onError: (message: string) => void; autoFocus?: boolean }) {
  const [choosing, setChoosing] = useState(false);
  const choose = async () => {
    setChoosing(true);
    try {
      const selected = await open({ title: "选择" + label, directory: true, multiple: false, defaultPath: value || undefined });
      if (typeof selected === "string") onChange(selected);
    } catch (cause) { onError(errorMessage(cause)); }
    finally { setChoosing(false); }
  };
  return <label className="field"><span>{label}</span><div className="path-row export-directory-input"><input aria-label={label} required value={value} autoFocus={autoFocus} disabled={disabled || choosing} placeholder="未设置" onChange={(event) => onChange(event.target.value)} /><button type="button" disabled={disabled || choosing} aria-label={"浏览" + label} title={"浏览" + label} onClick={() => void choose()}>{choosing ? <LoaderCircle size={16} className="spin" /> : <FolderOpen size={16} />}浏览</button></div></label>;
}

export function ExportSourceEditor({ componentId, projectPath, disabled = false, onBusy, onNotice, onSaved, onCancel }: { componentId: string; projectPath: string; disabled?: boolean; onBusy: (busy: boolean) => void; onNotice: (message: string) => void; onSaved?: (path: string) => Promise<void>; onCancel?: () => void }) {
  const [path, setPath] = useState(projectPath);
  const [info, setInfo] = useState<ExportSourceInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const edited = useRef(false);
  const pending = useRef(false);
  const revision = useRef(0);
  const load = useCallback(async () => {
    const current = ++revision.current;
    setLoading(true);
    try {
      const value = await api.exportSource(componentId);
      if (current !== revision.current) return;
      setInfo(value);
      if (!edited.current) setPath(value.path);
      setError("");
    } catch (cause) { if (current === revision.current) setError(errorMessage(cause)); }
    finally { if (current === revision.current) setLoading(false); }
  }, [componentId]);
  useEffect(() => {
    void load();
    const changed = nativeWindows ? listen<WorkspaceChange>(WORKSPACE_CHANGED, ({ payload }) => {
      if (payload.operation?.component?.id === componentId) void load();
    }) : null;
    return () => { revision.current += 1; void changed?.then((stop) => stop()); };
  }, [load, componentId]);
  const change = (value: string) => { edited.current = true; setPath(value); setError(""); };
  const save = async () => {
    if (pending.current || disabled || !path.trim()) return;
    pending.current = true; setSaving(true); onBusy(true); setError("");
    try {
      const saved = await api.setExportSource(componentId, path.trim());
      revision.current += 1;
      edited.current = false; setPath(saved); setInfo({ path: saved, configured: true, valid: true, issue: null });
      let synchronized = true;
      try {
        const component = await api.component(componentId);
        await notifyWorkspace(undefined, { component, actual_path: projectPath, modified_files: [], warnings: [] }, false);
      } catch (cause) { synchronized = false; onNotice("包体位置已保存，组件同步失败：" + errorMessage(cause)); }
      if (onSaved) await onSaved(saved); else if (synchronized) onNotice("包体位置已保存");
    } catch (cause) { setError(errorMessage(cause)); }
    finally { pending.current = false; setSaving(false); onBusy(false); }
  };
  const dirty = info ? path !== info.path : edited.current;
  const status = loading ? "正在识别包体" : dirty ? "尚未保存" : info?.valid ? (info.configured ? "已配置 · 已识别包体" : "自动使用项目目录 · 已识别包体") : "未识别到包体";
  return <div className="export-source-editor">
    <DirectoryInput label="包体位置" value={path} onChange={change} disabled={disabled || saving} onError={setError} autoFocus={!!onCancel} />
    <div className={"export-source-status" + (!loading && !dirty && !info?.valid ? " invalid" : "")} role="status">{loading ? <LoaderCircle className="spin" size={14} /> : dirty ? <Save size={14} /> : info?.valid ? <Check size={14} /> : <TriangleAlert size={14} />}<span>{status}</span></div>
    {!loading && !dirty && info?.issue && <p className="export-source-issue">{info.issue}</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
    <div className="dialog-actions">
      {onCancel && <button type="button" className="button secondary" disabled={saving} onClick={onCancel}>取消</button>}
      {!info && !loading && <button type="button" className="button secondary" disabled={saving || disabled} onClick={() => void load()}>重新识别</button>}
      <button type="button" className="button primary" disabled={disabled || saving || loading || !path.trim() || (!onCancel && !dirty && !!info?.configured && info.valid)} onClick={() => void save()}>{saving ? <LoaderCircle className="spin" size={15} /> : <Save size={15} />}{saving ? "保存中…" : onCancel ? "保存并继续" : "保存包体位置"}</button>
    </div>
  </div>;
}

export function ExportDirectoryForm({ request, onSaved, onCancel, onBusy, onNotice }: { request: ExportDirectoryRequest; onSaved: (path: string) => Promise<void>; onCancel: () => void; onBusy: (busy: boolean) => void; onNotice: (message: string) => void }) {
  const [destination, setDestination] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const pending = useRef(false);
  const save = async () => {
    if (pending.current || !destination.trim()) return;
    pending.current = true; setSaving(true); onBusy(true); setError("");
    try {
      const saved = await api.setQuickExportDestination(destination.trim());
      await onSaved(saved.quick_export.destination ?? destination.trim());
    } catch (cause) { setError(errorMessage(cause)); }
    finally { pending.current = false; setSaving(false); onBusy(false); }
  };
  return <div className="dialog-form export-directory-form">
    <div className="export-project"><span>项目目录</span><p title={request.projectPath}>{request.projectPath}</p></div>
    {request.purpose === "source" ? <ExportSourceEditor componentId={request.componentId} projectPath={request.projectPath} onBusy={onBusy} onNotice={onNotice} onSaved={onSaved} onCancel={onCancel} /> : <form className="dialog-form" onSubmit={(event) => { event.preventDefault(); void save(); }}>
      <DirectoryInput label="导出目录" value={destination} onChange={(value) => { setDestination(value); setError(""); }} disabled={saving} onError={setError} autoFocus />
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" className="button secondary" disabled={saving} onClick={onCancel}>取消</button><button type="submit" className="button primary" disabled={saving || !destination.trim()}>{saving ? <LoaderCircle size={15} className="spin" /> : <Check size={15} />}{saving ? "保存中…" : "保存并继续导出"}</button></div>
    </form>}
  </div>;
}
