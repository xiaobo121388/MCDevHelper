import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, Trash2, X } from "lucide-react";
import type { ReactNode } from "react";
import { errorMessage } from "./api";
import { valueAt, type ConfigData } from "./mcdkEditorApi";

export type Change = (key: string, value: unknown) => void;
type Props = { data: ConfigData; change: Change };
export const defaults: ConfigData = {
  world_name: "MC_DEV_WORLD", world_folder_name: "MC_DEV_WORLD", world_seed: null, world_type: 1, game_mode: 1,
  included_mod_dirs: ["./"], auto_join_game: true, include_debug_mod: true, auto_hot_reload_mods: true,
  enable_cheats: true, keep_inventory: true, do_daylight_cycle: true, do_weather_cycle: true,
};

export function EditorField({ label, children }: { label: string; children: ReactNode }) {
  return <label className="mcdk-field"><span>{label}</span>{children}</label>;
}
export function Toggle({ label, name, data, change, fallback = false }: Props & { label: string; name: string; fallback?: boolean }) {
  return <label className="mcdk-check"><input type="checkbox" checked={Boolean(valueAt(data, name) ?? fallback)} onChange={e => change(name, e.target.checked)} /><span>{label}</span></label>;
}
function TextField({ label, name, data, change, fallback = "" }: Props & { label: string; name: string; fallback?: string }) {
  return <EditorField label={label}><input value={String(valueAt(data, name) ?? fallback)} onChange={e => change(name, e.target.value)} /></EditorField>;
}
function SelectField({ label, name, data, change, options, fallback = 0 }: Props & { label: string; name: string; options: string[]; fallback?: number }) {
  return <EditorField label={label}><select value={Number(valueAt(data, name) ?? fallback)} onChange={e => change(name, Number(e.target.value))}>{options.map((label, i) => <option key={i} value={i}>{label}</option>)}</select></EditorField>;
}

export function WorldFields(props: Props) {
  return <><h2>世界设置</h2><div className="mcdk-field-grid">
    <TextField {...props} name="world_name" label="世界名称" fallback="MC_DEV_WORLD" />
    <TextField {...props} name="world_folder_name" label="世界文件夹" fallback="MC_DEV_WORLD" />
    <EditorField label="世界种子"><input type="number" step="1" min={-Number.MAX_SAFE_INTEGER} max={Number.MAX_SAFE_INTEGER} value={String(props.data.world_seed ?? "")} placeholder="随机" onChange={e => props.change("world_seed", e.target.value === "" ? null : Number(e.target.value))} /></EditorField>
    <SelectField {...props} name="world_type" label="世界类型" options={["旧世界", "无限", "平坦"]} fallback={1} />
    <SelectField {...props} name="game_mode" label="游戏模式" options={["生存", "创造", "冒险", "旁观"]} fallback={1} />
  </div><h3>游戏规则</h3><div className="mcdk-check-grid">{[["enable_cheats", "允许作弊"], ["keep_inventory", "死亡不掉落"], ["do_daylight_cycle", "昼夜交替"], ["do_weather_cycle", "天气变化"]].map(([name, label]) => <Toggle key={name} {...props} name={name} label={label} fallback />)}</div>
    <h3>实验性玩法</h3><div className="mcdk-check-grid">{[["data_driven_biomes", "自定义生物群系"], ["upcoming_creator_features", "即将推出的创作者功能"], ["experimental_creator_cameras", "实验性摄像机"], ["gametest", "Beta API"], ["deferred_technical_preview", "延迟渲染技术预览"]].map(([name, label]) => <Toggle key={name} {...props} name={"experiment_options." + name} label={label} />)}</div></>;
}

export function LaunchFields(props: Props & { onError: (message: string) => void }) {
  const browse = async () => {
    try { const path = await open({ directory: false, multiple: false, filters: [{ name: "Minecraft", extensions: ["exe"] }] }); if (typeof path === "string") props.change("game_executable_path", path); }
    catch (e) { props.onError(errorMessage(e)); }
  };
  return <><h2>启动配置</h2><EditorField label="游戏程序"><span className="mcdk-input-action"><input value={String(props.data.game_executable_path ?? "")} onChange={e => props.change("game_executable_path", e.target.value)} placeholder="Minecraft.Windows.exe" /><button type="button" title="选择游戏程序" aria-label="选择游戏程序" onClick={() => void browse()}><FolderOpen size={16} /></button></span></EditorField>
    <div className="mcdk-field-grid"><TextField {...props} name="user_name" label="玩家名称" /><SelectField {...props} name="log_protocol" label="日志协议" options={["默认日志", "备用日志协议"]} /></div>
    <h3>启动选项</h3><div className="mcdk-check-grid">{[["reset_world", "每次重置世界", false], ["auto_join_game", "自动进入游戏", true], ["include_debug_mod", "加载调试 Mod", true]].map(([name, label, fallback]) => <Toggle key={String(name)} {...props} name={String(name)} label={String(label)} fallback={Boolean(fallback)} />)}</div>
    <h3>自动热重载</h3><div className="mcdk-check-grid">{[["mods", "Python / Mod"], ["ui", "JSON UI"], ["shaders", "Shader"], ["materials", "材质"], ["particles", "粒子"]].map(([name, label]) => <Toggle key={name} {...props} name={"auto_hot_reload_" + name} label={label} fallback={name === "mods"} />)}</div></>;
}

type ModDir = { path: string; enabled: boolean; hot_reload: boolean; [key: string]: unknown };
export function ModFields(props: Props & { onError: (message: string) => void }) {
  const raw = props.data.included_mod_dirs ?? ["./"];
  const dirs: ModDir[] = Array.isArray(raw) ? raw.map(v => typeof v === "string" ? { path: v, enabled: true, hot_reload: true } : { ...v, path: String(v?.path ?? ""), enabled: v?.enabled !== false, hot_reload: v?.hot_reload !== false }) : [];
  const update = (index: number, patch: Partial<ModDir>) => props.change("included_mod_dirs", dirs.map((v, i) => i === index ? { ...v, ...patch } : v));
  const browse = async (index: number) => { try { const path = await open({ directory: true, multiple: false }); if (typeof path === "string") update(index, { path }); } catch (e) { props.onError(errorMessage(e)); } };
  return <><div className="mcdk-section-heading"><h2>Mod 目录</h2><button type="button" title="添加 Mod 目录" aria-label="添加 Mod 目录" disabled={dirs.length >= 128} onClick={() => props.change("included_mod_dirs", [...dirs, { path: "./", enabled: true, hot_reload: true }])}><Plus size={17} /></button></div>
    <div className="mcdk-mod-list">{dirs.map((dir, i) => <div className="mcdk-mod-row" key={i}>
      <span className="mcdk-input-action"><input aria-label={"Mod 路径 " + (i + 1)} required value={dir.path} onChange={e => update(i, { path: e.target.value })} /><button type="button" aria-label={"选择 Mod 目录 " + (i + 1)} title="选择目录" onClick={() => void browse(i)}><FolderOpen size={16} /></button></span>
      <label><input type="checkbox" checked={dir.enabled} onChange={e => update(i, { enabled: e.target.checked })} />启用</label><label><input type="checkbox" checked={dir.hot_reload} onChange={e => update(i, { hot_reload: e.target.checked })} />热重载</label>
      <button type="button" aria-label={"移除 Mod 目录 " + (i + 1)} title="移除目录引用" onClick={() => props.change("included_mod_dirs", dirs.filter((_, n) => i !== n))}><Trash2 size={16} /></button>
    </div>)}</div>{!dirs.length && <p className="mcdk-empty">未配置 Mod 目录</p>}</>;
}

export function WindowFields(props: Props) {
  const opacity = valueAt(props.data, "window_style.opacity");
  const rgb = valueAt(props.data, "window_style.title_bar_color");
  const color = Array.isArray(rgb) && rgb.length === 3 ? "#" + rgb.map(v => Number(v).toString(16).padStart(2, "0")).join("") : "#ffffff";
  return <><h2>游戏窗口</h2><div className="mcdk-check-grid">{[["always_on_top", "窗口置顶"], ["hide_title_bar", "隐藏标题栏"], ["hide_taskbar_icon", "隐藏任务栏图标"]].map(([name, label]) => <Toggle {...props} key={name} name={"window_style." + name} label={label} />)}</div>
    <div className="mcdk-field-grid"><EditorField label="不透明度"><span className="mcdk-input-action"><input type="checkbox" aria-label="设置不透明度" checked={opacity != null} onChange={e => props.change("window_style.opacity", e.target.checked ? 255 : null)} /><input type="range" min={0} max={255} value={Number(opacity ?? 255)} disabled={opacity == null} onChange={e => props.change("window_style.opacity", Number(e.target.value))} /><output>{opacity == null ? "默认" : String(opacity)}</output></span></EditorField>
    <EditorField label="标题栏颜色"><span className="mcdk-input-action"><input type="color" value={color} onChange={e => props.change("window_style.title_bar_color", [1, 3, 5].map(n => parseInt(e.target.value.slice(n, n + 2), 16)))} /><button type="button" title="恢复默认颜色" aria-label="恢复默认颜色" onClick={() => props.change("window_style.title_bar_color", null)}><X size={16} /></button></span></EditorField>
    <VectorField {...props} name="window_style.fixed_size" label="固定大小" axes={["宽", "高"]} minimum={1} />
    <VectorField {...props} name="window_style.fixed_position" label="固定位置" axes={["X", "Y"]} minimum={-32768} />
    <EditorField label="固定角落"><select value={Number(valueAt(props.data, "window_style.lock_corner") ?? 0)} onChange={e => props.change("window_style.lock_corner", Number(e.target.value) || null)}>{["不固定", "左上", "右上", "左下", "右下"].map((label, i) => <option value={i} key={i}>{label}</option>)}</select></EditorField></div></>;
}

function VectorField(props: Props & { name: string; label: string; axes: string[]; minimum: number }) {
  const value = valueAt(props.data, props.name);
  return <EditorField label={props.label}><span className="mcdk-input-action">{props.axes.map((axis, i) => <input key={axis} aria-label={props.label + axis} placeholder={axis} type="number" min={props.minimum} max={32768} step="1" value={Array.isArray(value) ? value[i] : ""} onChange={e => {
    const next = Array.isArray(value) ? [...value] : props.minimum > 0 ? [1280, 720] : [0, 0]; next[i] = Number(e.target.value); props.change(props.name, next);
  }} />)}<button type="button" aria-label={"清除" + props.label} title={"清除" + props.label} onClick={() => props.change(props.name, null)}><X size={16} /></button></span></EditorField>;
}

export function AdvancedFields(props: Props) {
  return <><h2>调试配置</h2><h3>热重载按键</h3><div className="mcdk-field-grid">{[["reload_key", "脚本重载"], ["reload_world_key", "世界重载"], ["reload_addon_key", "Addon 重载"], ["reload_shaders_key", "Shader 重载"]].map(([key, label]) => <EditorField key={key} label={label}><span className="mcdk-input-action"><input aria-label={label + "按键"} value={String(valueAt(props.data, "debug_options." + key) ?? "")} placeholder="未设置" readOnly onKeyDown={e => { if (e.key === "Tab" || e.key === "Escape") return; e.preventDefault(); props.change("debug_options." + key, String(e.keyCode)); }} /><button type="button" title={"清除" + label + "按键"} aria-label={"清除" + label + "按键"} onClick={() => props.change("debug_options." + key, "")}><X size={16} /></button></span></EditorField>)}</div>
    <Toggle {...props} name="debug_options.reload_key_global" label="全局重载按键" />
    <h3>MCDK MCP 服务</h3><Toggle {...props} name="mcp_server_config.enabled" label="启用 MCP 服务" />
    <div className="mcdk-field-grid"><TextField {...props} name="mcp_server_config.server_ip" label="监听地址" fallback="localhost" /><EditorField label="监听端口"><input type="number" min={1} max={65535} required value={Number(valueAt(props.data, "mcp_server_config.server_port") ?? 19133)} onChange={e => props.change("mcp_server_config.server_port", Number(e.target.value))} /></EditorField></div></>;
}
