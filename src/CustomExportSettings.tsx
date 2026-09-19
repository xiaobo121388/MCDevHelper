import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, FolderOpen, Plus, Save, Trash2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorMessage } from "./api";
import type { ComponentKind, CustomExportProfile } from "./types";

const kinds: [ComponentKind, string][] = [["addon", "模组"], ["map", "地图"], ["material", "材质"]];
const fresh = (): CustomExportProfile => ({
  id: crypto.randomUUID(), name: "自定义导出", enabled: true, executable: "", arguments: [],
  input_mode: "snapshot", working_directory: null, component_kinds: ["addon", "map", "material"],
  timeout_seconds: 1800, log_encoding: "utf8", allow_mcp: false,
});
const executionChanged = (left: CustomExportProfile, right?: CustomExportProfile) => !right
  || left.executable !== right.executable || JSON.stringify(left.arguments) !== JSON.stringify(right.arguments)
  || left.input_mode !== right.input_mode || left.working_directory !== right.working_directory;

export function CustomExportSettings() {
  const [profiles, setProfiles] = useState<CustomExportProfile[]>([]);
  const [saved, setSaved] = useState<CustomExportProfile[]>([]);
  const [selected, setSelected] = useState("");
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    api.customExportProfiles().then((value) => {
      if (active) { setProfiles(value); setSaved(value); setSelected(value[0]?.id ?? ""); }
    }).catch((failure) => { if (active) { setError(errorMessage(failure)); setLoadFailed(true); } })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);
  const profile = profiles.find((item) => item.id === selected);
  const original = saved.find((item) => item.id === selected);
  const update = (changes: Partial<CustomExportProfile>) => {
    setMessage("");
    setProfiles((items) => items.map((item) => {
      if (item.id !== selected) return item;
      const next = { ...item, ...changes };
      if (executionChanged(next, item)) next.allow_mcp = false;
      return next;
    }));
  };
  const choosePath = async (directory: boolean) => {
    try {
      const path = await open({ directory, multiple: false, ...(!directory ? { filters: [{ name: "可执行程序", extensions: ["exe"] }] } : {}) });
      if (typeof path === "string") update(directory ? { working_directory: path } : { executable: path });
    } catch (failure) { setError(errorMessage(failure)); }
  };
  const add = () => { const next = fresh(); setProfiles([...profiles, next]); setSelected(next.id); setMessage(""); };
  const remove = (item: CustomExportProfile) => {
    if (!window.confirm('删除导出方案“' + item.name + '”？')) return;
    const next = profiles.filter((candidate) => candidate.id !== item.id);
    setProfiles(next); if (selected === item.id) setSelected(next[0]?.id ?? ""); setMessage("");
  };
  const move = (index: number, delta: number) => {
    const next = [...profiles]; [next[index], next[index + delta]] = [next[index + delta], next[index]];
    setProfiles(next); setMessage("");
  };
  const save = async () => {
    const risky = profiles.some((item) => item.input_mode === "source" && executionChanged(item, saved.find((old) => old.id === item.id)));
    if (risky && !window.confirm("原目录模式会允许外部程序直接处理项目文件，修改或删除无法撤销。确认保存？")) return;
    setBusy(true); setError(""); setMessage("");
    try {
      const next = await api.saveCustomExportProfiles(profiles);
      setProfiles(next); setSaved(next); setMessage("导出方案已保存。");
    } catch (failure) { setError(errorMessage(failure)); }
    finally { setBusy(false); }
  };
  return <section className="custom-export-settings" aria-label="自定义导出方案">
    <div className="section-heading"><h3>自定义导出</h3><button className="button secondary" disabled={loading || busy || loadFailed} onClick={add}><Plus size={16} />新增方案</button></div>
    {loading && <p role="status">正在加载…</p>}
    <fieldset disabled={loading || busy || loadFailed} className="export-config-fields">
      <div className="export-profile-list" role="list" aria-label="导出方案列表">{profiles.map((item, index) => <div role="listitem" className="export-profile-row" key={item.id}>
        <input type="checkbox" aria-label={'启用 ' + item.name} checked={item.enabled} onChange={(event) => setProfiles(profiles.map((p) => p.id === item.id ? { ...p, enabled: event.target.checked } : p))} />
        <button className={selected === item.id ? "export-profile-name active" : "export-profile-name"} onClick={() => setSelected(item.id)} title={item.name}>{item.name}</button>
        <button className="export-icon" title="上移" aria-label={'上移 ' + item.name} disabled={index === 0} onClick={() => move(index, -1)}><ArrowUp size={15} /></button>
        <button className="export-icon" title="下移" aria-label={'下移 ' + item.name} disabled={index === profiles.length - 1} onClick={() => move(index, 1)}><ArrowDown size={15} /></button>
        <button className="export-icon" title="删除方案" aria-label={'删除 ' + item.name} onClick={() => remove(item)}><Trash2 size={15} /></button>
      </div>)}</div>
      {!loading && !profiles.length && <p className="source-empty">尚未配置自定义导出方案。</p>}
      {profile && <div className="export-profile-editor">
        <label className="field"><span>按钮名称</span><input maxLength={60} value={profile.name} onChange={(event) => update({ name: event.target.value })} /></label>
        <label className="field"><span>程序</span><div className="path-row"><input value={profile.executable} placeholder="C:\Tools\python.exe" onChange={(event) => update({ executable: event.target.value })} /><button className="export-icon" title="选择程序" type="button" aria-label="选择打包程序" onClick={() => void choosePath(false)}><FolderOpen size={17} /></button></div></label>
        <div className="export-arguments"><div className="export-field-heading"><strong>参数</strong><button className="export-icon" title="添加参数" aria-label="添加参数" onClick={() => update({ arguments: [...profile.arguments, ""] })}><Plus size={16} /></button></div>
          {profile.arguments.map((value, index) => <div className="path-row" key={index}><input aria-label={'参数 ' + (index + 1)} value={value} title="{input_dir} · {output_dir} · {work_dir} · {component_id} · {component_name} · {component_kind}" onChange={(event) => update({ arguments: profile.arguments.map((arg, at) => at === index ? event.target.value : arg) })} /><button className="export-icon" title="移除参数" aria-label={'移除参数 ' + (index + 1)} onClick={() => update({ arguments: profile.arguments.filter((_, at) => at !== index) })}><X size={15} /></button></div>)}
        </div>
        <div className="settings-grid">
          <label className="field"><span>输入方式</span><select value={profile.input_mode} onChange={(event) => update({ input_mode: event.target.value as CustomExportProfile["input_mode"] })}><option value="snapshot">完整临时副本</option><option value="source">原始组件目录</option></select></label>
          <label className="field"><span>超时（秒）</span><input type="number" min={1} max={4294967295} step={1} value={profile.timeout_seconds} onChange={(event) => update({ timeout_seconds: Number(event.target.value) })} /></label>
          <label className="field"><span>日志编码</span><select value={profile.log_encoding} onChange={(event) => update({ log_encoding: event.target.value as CustomExportProfile["log_encoding"] })}><option value="utf8">UTF-8</option><option value="gb18030">GB18030</option></select></label>
        </div>
        <label className="field"><span>工作目录</span><div className="path-row"><input value={profile.working_directory ?? ""} placeholder="默认使用输入目录" onChange={(event) => update({ working_directory: event.target.value || null })} /><button className="export-icon" title="选择工作目录" type="button" aria-label="选择工作目录" onClick={() => void choosePath(true)}><FolderOpen size={17} /></button></div></label>
        <div className="export-kind-options" role="group" aria-label="适用类型">{kinds.map(([kind, label]) => <label key={kind}><input type="checkbox" checked={profile.component_kinds.includes(kind)} onChange={(event) => update({ component_kinds: event.target.checked ? [...profile.component_kinds, kind] : profile.component_kinds.filter((item) => item !== kind) })} />{label}</label>)}</div>
        <label className="export-mcp-option"><input type="checkbox" checked={profile.allow_mcp} disabled={executionChanged(profile, original)} onChange={(event) => {
          if (!event.target.checked || window.confirm("允许 AI/MCP 运行此方案配置的本机程序？程序拥有当前用户权限。")) update({ allow_mcp: event.target.checked });
        }} />允许 MCP 执行</label>
        {executionChanged(profile, original) && <p className="export-security-note">执行配置尚未保存，MCP 授权已关闭。</p>}
        {profile.input_mode === "source" && <p className="export-security-note">原目录模式：项目文件可能被修改或删除，无法撤销。</p>}
        <p className="export-security-note">外部程序拥有当前用户权限。临时副本不是安全沙箱。</p>
      </div>}
    </fieldset>
    {error && <p className="form-error" role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
    <div className="export-config-footer"><button className="button primary" disabled={loading || busy || loadFailed} onClick={() => void save()}><Save size={16} />{busy ? "保存中…" : "保存导出方案"}</button></div>
  </section>;
}
