import { open } from "@tauri-apps/plugin-dialog";
import { Box, FolderOpen, Image, Plus, RefreshCw, Trash2, UserRound } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "./api";
import { editorApi, legacyVanillaMatch, normalizeSkinPath, valueAt, type ConfigData, type CustomSkin, type SkinCatalog } from "./mcdkEditorApi";
import { EditorField, Toggle, type Change } from "./McdkConfigFields";
import { McdkChoice } from "./McdkChoice";

const filename = (path: string) => path.replaceAll("\\", "/").split("/").at(-1) || path;
export function McdkSkinPicker({ componentId, data, change, onBusy, disabled }: { componentId: string; data: ConfigData; change: Change; onBusy: (busy: boolean) => void; disabled?: boolean }) {
  const game = String(data.game_executable_path ?? "");
  const selected = String(valueAt(data, "skin_info.skin") ?? "");
  const slim = Boolean(valueAt(data, "skin_info.slim"));
  const [catalog, setCatalog] = useState<SkinCatalog>({ skins: [], warnings: [] });
  const [custom, setCustom] = useState<CustomSkin[]>([]);
  const [catalogError, setCatalogError] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [path, setPath] = useState("");
  const [working, setWorking] = useState(false);
  const libraryRequest = useRef(0);
  const [preview, setPreview] = useState({ url: "", error: "", loading: false });

  useEffect(() => {
    let active = true;
    setCatalog({ skins: [], warnings: [] }); setCatalogError("");
    if (!game.trim()) { setLoading(false); return; }
    setLoading(true);
    const timer = setTimeout(() => {
      void editorApi.catalog(game).then(value => { if (active) setCatalog(value); }).catch(e => { if (active) setCatalogError(errorMessage(e)); }).finally(() => { if (active) setLoading(false); });
    }, 250);
    return () => { active = false; clearTimeout(timer); };
  }, [game, refresh]);
  useEffect(() => {
    let active = true;
    const request = ++libraryRequest.current;
    void editorApi.library(componentId).then(value => { if (active && request === libraryRequest.current) setCustom(value); }).catch(e => { if (active && request === libraryRequest.current) setError(errorMessage(e)); });
    return () => { active = false; };
  }, [componentId]);
  const match = catalog.skins.find(s => normalizeSkinPath(s.path) === normalizeSkinPath(selected));
  const migrated = !match ? legacyVanillaMatch(selected, game, catalog.skins) : undefined;
  const defaultSteve = catalog.skins.find(s => /steve/i.test(s.name + " " + s.texture));
  const previewPath = selected || defaultSteve?.path || "";
  useEffect(() => {
    let active = true;
    setPreview({ url: "", error: "", loading: !!previewPath });
    if (!previewPath) return;
    const timer = setTimeout(() => {
      void editorApi.preview(componentId, previewPath).then(url => { if (active) setPreview({ url, error: "", loading: false }); }).catch(e => { if (active) setPreview({ url: "", error: errorMessage(e), loading: false }); });
    }, 150);
    return () => { active = false; clearTimeout(timer); };
  }, [componentId, previewPath]);

  const select = (path: string, slim: boolean) => { change("skin_info.skin", path); change("skin_info.slim", slim); };
  const mutate = async (path: string, remove = false, model = slim) => {
    if (working || !path.trim()) return;
    libraryRequest.current += 1;
    setWorking(true); onBusy(true); setError("");
    try {
      const next = await editorApi.updateLibrary(componentId, path, model, remove);
      setCustom(next);
      if (!remove) { const added = next.at(-1); if (added) select(added.path, added.slim); setPath(""); }
    } catch (e) { setError(errorMessage(e)); }
    finally { setWorking(false); onBusy(false); }
  };
  const browse = async () => {
    try { const picked = await open({ directory: false, multiple: false, filters: [{ name: "PNG skin", extensions: ["png"] }] }); if (typeof picked === "string") await mutate(picked); }
    catch (e) { setError(errorMessage(e)); }
  };
  const known = match || custom.find(s => normalizeSkinPath(s.path) === normalizeSkinPath(selected));
  return <><div className="mcdk-section-heading"><h2>玩家皮肤</h2><button type="button" title="刷新皮肤目录" aria-label="刷新皮肤目录" disabled={loading} onClick={() => setRefresh(v => v + 1)}><RefreshCw size={16} className={loading ? "spin" : ""} /></button></div>
    <div className="mcdk-skin-layout"><div className="mcdk-skin-controls"><EditorField label="皮肤选择"><McdkChoice label="皮肤选择" value={known?.path ?? selected} disabled={disabled || working} onChange={path => { const skin = catalog.skins.find(s => s.path === path) || custom.find(s => s.path === path); select(path, skin?.slim ?? false); }} groups={[
      { items: [{ value: "", label: "默认 Steve", badge: "Classic", icon: <UserRound size={17} /> }, ...(selected && !known ? [{ value: selected, label: filename(selected), badge: "当前配置", description: selected, icon: <Image size={17} /> }] : [])] },
      { label: "原版免费皮肤", items: catalog.skins.filter(s => s.kind === "vanilla").map(s => ({ value: s.path, label: s.name, badge: s.slim ? "Slim" : "Classic", icon: <UserRound size={17} /> })) },
      { label: "网易 4D", items: catalog.skins.filter(s => s.kind === "netease4d").map(s => ({ value: s.path, label: s.name, badge: "4D", icon: <Box size={17} /> })) },
      { label: "自定义皮肤库", items: custom.filter(c => !catalog.skins.some(s => normalizeSkinPath(s.path) === normalizeSkinPath(c.path))).map(s => ({ value: s.path, label: filename(s.path), badge: s.slim ? "Slim" : "Classic", description: s.path, icon: <Image size={17} /> })) },
    ]} /></EditorField>
    {loading && <p role="status">正在读取皮肤目录</p>}
    {!game && <p className="mcdk-empty">尚未选择游戏程序</p>}
    {catalogError && <p role="alert" className="mcdk-editor-error">{catalogError}</p>}
    {catalog.warnings.map(w => <p role="alert" className="mcdk-editor-error" key={w}>{w}</p>)}
    {migrated && <div className="mcdk-editor-warning">当前皮肤来自旧游戏版本。<button type="button" onClick={() => select(migrated.path, migrated.slim)}>切换至当前版本</button></div>}
    <EditorField label="皮肤路径"><input value={selected} onChange={e => change("skin_info.skin", e.target.value)} placeholder="默认皮肤" /></EditorField>
    {!match && <Toggle data={data} change={(key, value) => {
      change(key, value);
      const saved = custom.find(s => normalizeSkinPath(s.path) === normalizeSkinPath(selected));
      if (saved) void mutate(saved.path, false, Boolean(value));
    }} name="skin_info.slim" label="Slim 模型（三像素手臂）" />}
    {match && <p className="mcdk-skin-model">{match.kind === "netease4d" ? "网易 4D" : match.slim ? "Slim" : "Classic"}</p>}
    </div><figure className="mcdk-skin-preview"><div>{preview.loading ? <span role="status">正在加载预览</span> : preview.url ? <img src={preview.url} alt={(match?.name || filename(selected) || "Steve") + "皮肤预览"} onError={() => setPreview({ url: "", error: "皮肤图像无法显示", loading: false })} /> : <UserRound size={64} strokeWidth={1} />}</div><figcaption>{match?.name || filename(selected) || "默认 Steve"}</figcaption></figure></div>
    {preview.error && <p role="alert" className="mcdk-editor-error">{preview.error}</p>}
    <h3>自定义皮肤库 <span>{custom.length}/64</span></h3>
    <div className="mcdk-input-action"><input aria-label="添加皮肤路径" value={path} onChange={e => setPath(e.target.value)} placeholder="PNG 文件路径" /><button type="button" disabled={working || !path.trim()} title="加入皮肤库" aria-label="加入皮肤库" onClick={() => void mutate(path)}><Plus size={16} /></button><button type="button" disabled={working} title="选择 PNG 皮肤" aria-label="选择 PNG 皮肤" onClick={() => void browse()}><FolderOpen size={16} /></button></div>
    {error && <p className="mcdk-editor-error" role="alert">{error}</p>}
    <ul className="mcdk-skin-library">{custom.map(s => <li key={s.path}><button type="button" className="mcdk-skin-name" title={s.path} onClick={() => select(s.path, s.slim)}>{filename(s.path)}<small>{s.slim ? "Slim" : "Classic"}</small></button><button type="button" disabled={working} title={"从皮肤库移除 " + filename(s.path)} aria-label={"从皮肤库移除 " + filename(s.path)} onClick={() => void mutate(s.path, true)}><Trash2 size={15} /></button></li>)}</ul>
  </>;
}
