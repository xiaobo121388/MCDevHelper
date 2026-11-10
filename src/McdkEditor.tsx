import { getCurrentWindow } from "@tauri-apps/api/window";
import { Boxes, Globe2, Keyboard, LoaderCircle, Monitor, Play, RefreshCw, Save, Settings2, Shirt, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { api, errorMessage } from "./api";
import { confirmAction, nativeWindows } from "./windows";
import { editorApi, applyEdits, valueAt, type ConfigEdit, type ConfigSnapshot } from "./mcdkEditorApi";
import { AdvancedFields, defaults, LaunchFields, ModFields, WindowFields, WorldFields } from "./McdkConfigFields";
import { McdkSkinPicker } from "./McdkSkinPicker";
import "./mcdkEditor.css";

const tabs = [
  { id: "launch", label: "启动", icon: Settings2 }, { id: "world", label: "世界", icon: Globe2 },
  { id: "mods", label: "Mod 目录", icon: Boxes }, { id: "skin", label: "皮肤", icon: Shirt },
  { id: "window", label: "窗口", icon: Monitor }, { id: "debug", label: "调试", icon: Keyboard },
] as const;

export function McdkEditor({ componentId, onClose, running = false, canLaunch = false }: { componentId: string; onClose: () => void; running?: boolean; canLaunch?: boolean }) {
  const [snapshot, setSnapshot] = useState<ConfigSnapshot | null>(null);
  const [edits, setEdits] = useState<Record<string, ConfigEdit>>({});
  const [tab, setTab] = useState<string>("launch");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [skinBusy, setSkinBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const request = useRef(0);
  const discardApproved = useRef(false);
  const dirty = Object.keys(edits).length > 0;
  const locked = busy || skinBusy;
  const data = useMemo(() => applyEdits(snapshot?.data ?? {}, Object.values(edits)), [snapshot, edits]);
  const load = useCallback(async () => {
    const id = ++request.current; setLoading(true); setError(""); setNotice("");
    try { const result = await editorApi.load(componentId); if (id === request.current) { setSnapshot(result); setEdits({}); } }
    catch (e) { if (id === request.current) setError(errorMessage(e)); }
    finally { if (id === request.current) setLoading(false); }
  }, [componentId]);
  useEffect(() => { void load(); return () => { request.current += 1; }; }, [load]);
  const close = useCallback(async () => {
    if (locked) return;
    if (dirty && !await confirmAction("配置尚未保存，确定放弃修改并关闭？")) return;
    discardApproved.current = true;
    onClose();
  }, [dirty, locked, onClose]);
  useEffect(() => {
    if (!nativeWindows) return;
    const stop = getCurrentWindow().onCloseRequested(async event => {
      if (discardApproved.current) return;
      if (locked) { event.preventDefault(); return; }
      if (dirty) {
        event.preventDefault();
        if (await confirmAction("配置尚未保存，确定放弃修改并关闭？")) {
          discardApproved.current = true;
          await getCurrentWindow().close().catch(e => { discardApproved.current = false; setError(errorMessage(e)); });
        }
      }
    });
    return () => { void stop.then(dispose => dispose()); };
  }, [dirty, locked]);
  useEffect(() => {
    const guard = (event: BeforeUnloadEvent) => { if (!discardApproved.current && (dirty || locked)) { event.preventDefault(); event.returnValue = ""; } };
    window.addEventListener("beforeunload", guard); return () => window.removeEventListener("beforeunload", guard);
  }, [dirty, locked]);
  const change = (key: string, value: unknown) => {
    setNotice("");
    setEdits(previous => {
      const next = { ...previous };
      if (JSON.stringify(valueAt(snapshot?.data ?? {}, key)) === JSON.stringify(value)) delete next[key];
      else next[key] = { path: key.split("."), value };
      return next;
    });
  };
  const reload = async () => { if (!dirty || await confirmAction("重新加载会放弃尚未保存的配置，是否继续？")) await load(); };
  const save = async (launch: boolean) => {
    if (!snapshot || locked || running) return;
    setBusy(true); setError(""); setNotice("");
    try {
      const changes = snapshot.revision === null ? { ...Object.fromEntries(Object.entries(defaults).map(([key, value]) => [key, { path: [key], value }])), ...edits } : edits;
      // Even launch without changes checks the source revision before starting a process.
      const saved = await editorApi.save(componentId, snapshot.revision, Object.values(changes));
      setSnapshot(saved); setEdits({}); setNotice("配置已保存");
      if (launch) { await api.launchGame(componentId); setNotice("游戏启动请求已提交"); }
    } catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const submitter = (event.nativeEvent as SubmitEvent).submitter;
    void save(submitter instanceof HTMLButtonElement && submitter.value === "launch");
  };
  const form = useRef<HTMLFormElement>(null);
  useEffect(() => {
    const handler = (event: KeyboardEvent) => { if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") { event.preventDefault(); if (!locked && !running && dirty) form.current?.requestSubmit(); } };
    document.addEventListener("keydown", handler); return () => document.removeEventListener("keydown", handler);
  }, [locked, running, dirty]);
  return <form className="mcdk-editor" ref={form} onSubmit={submit}>
    <header className="mcdk-editor-heading"><div><h1>MCDK 可视化配置</h1><p title={snapshot?.path}>{snapshot?.path ?? componentId}</p></div><button type="button" title="关闭配置" aria-label="关闭配置" onClick={() => void close()} disabled={locked}><X size={19} /></button></header>
    <div className="mcdk-editor-toolbar"><span role="status">{loading ? "正在加载" : running ? "游戏运行中 · 只读" : dirty ? "有未保存的修改" : snapshot?.revision ? "已保存" : "新配置"}</span><div>
      <button type="button" title="重新加载配置" aria-label="重新加载配置" disabled={locked || loading} onClick={() => void reload()}><RefreshCw size={16} /></button>
      <button type="submit" className="button secondary" disabled={locked || loading || running || !snapshot || (!dirty && !!snapshot.revision)}>{busy ? <LoaderCircle size={16} className="spin" /> : <Save size={16} />}保存</button>
      <button type="submit" value="launch" className="button primary" disabled={locked || loading || running || !snapshot || !canLaunch}><Play size={16} />保存并启动</button>
    </div></div>
    {error && <div className="mcdk-editor-error" role="alert">{error}</div>}{notice && <div className="mcdk-editor-notice" role="status">{notice}</div>}
    <div className="mcdk-editor-body"><nav className="mcdk-editor-tabs" aria-label="配置分类">{tabs.map(({ id, label, icon: Icon }) => <button type="button" key={id} aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)}><Icon size={17} /><span>{label}</span></button>)}</nav>
      <fieldset className="mcdk-editor-content" disabled={locked || running || loading || !snapshot}>
        {loading ? <div className="mcdk-empty" role="status">正在读取 .mcdev.json</div> : snapshot && <>
          <section hidden={tab !== "launch"}><LaunchFields data={data} change={change} onError={setError} /></section>
          <section hidden={tab !== "world"}><WorldFields data={data} change={change} /></section>
          <section hidden={tab !== "mods"}><ModFields data={data} change={change} onError={setError} /></section>
          <section hidden={tab !== "skin"}><McdkSkinPicker componentId={componentId} data={data} change={change} onBusy={setSkinBusy} /></section>
          <section hidden={tab !== "window"}><WindowFields data={data} change={change} /></section>
          <section hidden={tab !== "debug"}><AdvancedFields data={data} change={change} /></section>
        </>}
      </fieldset>
    </div>
  </form>;
}
