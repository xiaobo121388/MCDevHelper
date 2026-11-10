import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Monitor, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "./api";
import { editorApi, normalizeSkinPath, type GameCatalog } from "./mcdkEditorApi";
import { McdkChoice } from "./McdkChoice";
import { EditorField, type Change } from "./McdkConfigFields";

export function McdkGamePicker({ componentId, value, change, disabled }: { componentId: string; value: string; change: Change; disabled?: boolean }) {
  const [catalog, setCatalog] = useState<GameCatalog>({ installations: [], warnings: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
    setLoading(true); setError(""); setCatalog({ installations: [], warnings: [] });
    const timer = setTimeout(() => {
      void editorApi.games(componentId, value).then(result => { if (active) setCatalog(result); })
        .catch(e => { if (active) setError(errorMessage(e)); }).finally(() => { if (active) setLoading(false); });
    }, 250);
    return () => { active = false; clearTimeout(timer); };
  }, [componentId, value, refresh]);
  const selected = catalog.installations.find(item => normalizeSkinPath(item.path) === normalizeSkinPath(catalog.selected_path ?? value));
  const browse = async () => {
    try {
      const path = await open({ directory: false, multiple: false, filters: [{ name: "Minecraft", extensions: ["exe"] }] });
      if (typeof path === "string") change("game_executable_path", path);
    } catch (e) { setError(errorMessage(e)); }
  };
  return <div className="mcdk-game-picker">
    <EditorField label="游戏版本"><span className="mcdk-input-action">
      <McdkChoice label="游戏版本" value={selected?.path ?? value} disabled={disabled} onChange={path => change("game_executable_path", path)} groups={[
        { items: [{ value: "", label: "未指定版本", icon: <Monitor size={17} /> }, ...(value && !selected ? [{ value, label: "当前配置", badge: loading ? "检测中" : "未检测到", description: value, icon: <Monitor size={17} /> }] : [])] },
        { label: "本机游戏", items: catalog.installations.map(item => ({ value: item.path, label: item.version ?? "自定义程序", badge: { mcstudio: "MCStudio", sibling: "本地", configured: "自定义" }[item.source], description: item.path, icon: <Monitor size={17} /> })) },
      ]} />
      <button type="button" aria-label="刷新游戏版本" title="刷新游戏版本" disabled={disabled || loading} onClick={() => setRefresh(v => v + 1)}><RefreshCw size={16} className={loading ? "spin" : ""} /></button>
    </span></EditorField>
    <EditorField label="游戏程序"><span className="mcdk-input-action"><input aria-label="游戏程序" value={value} onChange={e => change("game_executable_path", e.target.value)} placeholder="Minecraft.Windows.exe" /><button type="button" title="选择游戏程序" aria-label="选择游戏程序" disabled={disabled} onClick={() => void browse()}><FolderOpen size={16} /></button></span></EditorField>
    {loading && <p className="mcdk-field-status" role="status">正在检测本机游戏版本</p>}
    {!loading && !catalog.installations.length && !error && <p className="mcdk-field-status">未发现已安装的游戏版本</p>}
    {error && <p role="alert" className="mcdk-editor-error">{error}</p>}
    {catalog.warnings.map(warning => <p key={warning} role="alert" className="mcdk-editor-warning">{warning}</p>)}
  </div>;
}
