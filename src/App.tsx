import { open } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  Boxes,
  Box,
  ChevronRight,
  Code2,
  Copy,
  Folder,
  FolderCog,
  FolderOpen,
  Hash,
  Import,
  Map,
  MoreHorizontal,
  PackagePlus,
  Palette,
  RefreshCw,
  Search,
  Settings2,
  Sparkles,
  Tag,
  X,
} from "lucide-react";
import { FormEvent, ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { api, desktop, errorMessage } from "./api";
import type {
  ComponentKind,
  ComponentSummary,
  DiscoveryResult,
  IdentityPolicy,
  SourceRecord,
  VersionPart,
} from "./types";

const EMPTY_RESULT: DiscoveryResult = { components: [], sources: [], warnings: [] };
const kindText: Record<ComponentKind, string> = { addon: "模组", material: "材质", map: "地图" };

export function App() {
  const [result, setResult] = useState(EMPTY_RESULT);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | ComponentKind>("all");
  const [tag, setTag] = useState("all");
  const [modal, setModal] = useState<"create" | "import" | "sources" | null>(null);
  const [selected, setSelected] = useState<ComponentSummary | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!desktop) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      setResult(await api.refresh());
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 4200);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const tags = useMemo(
    () => [...new Set(result.components.flatMap((component) => component.tags))].sort(),
    [result.components],
  );
  const components = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return result.components.filter((component) => {
      if (kind !== "all" && component.kind !== kind) return false;
      if (tag !== "all" && !component.tags.includes(tag)) return false;
      return !needle || `${component.name}\n${component.path}`.toLocaleLowerCase().includes(needle);
    });
  }, [kind, query, result.components, tag]);

  const done = async (message: string) => {
    setNotice(message);
    await refresh();
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark"><Boxes size={20} /></span>
          <span><strong>MCDH</strong><small>MCDevHelper</small></span>
        </div>
        <nav className="category-nav" aria-label="组件分类">
          <NavItem active={kind === "all"} icon={<Boxes />} label="全部组件" count={result.components.length} onClick={() => setKind("all")} />
          <NavItem active={kind === "addon"} icon={<Box />} label="模组" count={countKind(result.components, "addon")} onClick={() => setKind("addon")} />
          <NavItem active={kind === "material"} icon={<Palette />} label="材质" count={countKind(result.components, "material")} onClick={() => setKind("material")} />
          <NavItem active={kind === "map"} icon={<Map />} label="地图" count={countKind(result.components, "map")} onClick={() => setKind("map")} />
        </nav>
        <div className="sidebar-spacer" />
        <button className="sidebar-action" onClick={() => setModal("sources")}>
          <FolderCog size={18} /><span>路径管理</span><ChevronRight size={15} />
        </button>
        <div className="offline-badge"><span /> 完全离线 · 本地数据</div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">创作组件</p>
            <h1>{kind === "all" ? "全部组件" : kindText[kind]}</h1>
          </div>
          <div className="top-actions">
            <button className="button secondary" onClick={() => setModal("import")}><Import size={17} />导入</button>
            <button className="button primary" onClick={() => setModal("create")}><PackagePlus size={17} />新建组件</button>
          </div>
        </header>

        <section className="toolbar" aria-label="筛选工具">
          <label className="search-box"><Search size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索名称或路径" /></label>
          <label className="select-box"><Tag size={16} /><select value={tag} onChange={(event) => setTag(event.target.value)}><option value="all">全部标签</option>{tags.map((value) => <option key={value} value={value}>{value}</option>)}</select></label>
          <button className="icon-button" aria-label="刷新组件" title="刷新组件" onClick={() => void refresh()}><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
          <span className="result-count">{components.length} 个组件</span>
        </section>

        {result.warnings.length > 0 && <div className="warning-line">有 {result.warnings.length} 个路径未能读取；打开路径管理后可检查来源。</div>}

        <section className="component-grid" aria-live="polite">
          {components.map((component) => <ComponentCard key={component.id} component={component} onOpen={() => setSelected(component)} onNotice={setNotice} />)}
          {!loading && components.length === 0 && (
            <div className="empty-state">
              <span><Boxes size={28} /></span>
              <h2>{result.components.length ? "没有匹配的组件" : "还没有发现组件"}</h2>
              <p>{result.components.length ? "调整搜索词、分类或标签筛选。" : "添加组件库、单个组件，或创建一个新项目。"}</p>
              {!result.components.length && <button className="button secondary" onClick={() => setModal("sources")}><FolderOpen size={17} />添加存放路径</button>}
            </div>
          )}
        </section>
      </main>

      {modal === "create" && <CreateDialog onClose={() => setModal(null)} onDone={(message) => { setModal(null); void done(message); }} />}
      {modal === "import" && <ImportDialog onClose={() => setModal(null)} onDone={(message) => { setModal(null); void done(message); }} />}
      {modal === "sources" && <SourcesDialog onClose={() => setModal(null)} onChanged={() => void refresh()} onNotice={setNotice} />}
      {selected && <ComponentDialog component={selected} onClose={() => setSelected(null)} onDone={(message) => { setSelected(null); void done(message); }} onNotice={setNotice} />}
      {notice && <div className="toast" role="status">{notice}</div>}
    </div>
  );
}

function NavItem({ active, icon, label, count, onClick }: { active: boolean; icon: ReactNode; label: string; count: number; onClick: () => void }) {
  return <button className={active ? "nav-item active" : "nav-item"} onClick={onClick}><span>{icon}</span>{label}<small>{count}</small></button>;
}

function ComponentCard({ component, onOpen, onNotice }: { component: ComponentSummary; onOpen: () => void; onNotice: (text: string) => void }) {
  const icon = component.kind === "addon" ? <Box /> : component.kind === "material" ? <Palette /> : <Map />;
  const openDirectory = async () => {
    try { await api.openDirectory(component.id); } catch (error) { onNotice(errorMessage(error)); }
  };
  const openCode = async () => {
    try { await api.openVsCode(component.id); } catch (error) { onNotice(errorMessage(error)); }
  };
  return (
    <article className="component-card">
      <div className={`component-icon ${component.kind}`}>{icon}</div>
      <div className="component-heading">
        <div><h2 title={component.name}>{component.name}</h2><p title={component.path}>{component.path}</p></div>
        <button className="card-menu" aria-label={`配置 ${component.name}`} onClick={onOpen}><MoreHorizontal size={19} /></button>
      </div>
      <div className="badges">
        <span>{kindText[component.kind]}</span>
        {component.version && <span>v{component.version.join(".")}</span>}
        {component.mcs && <span className="mcs-badge">MCS</span>}
        <span>{originText(component)}</span>
      </div>
      <div className="tag-row">{component.tags.length ? component.tags.map((value) => <span key={value}><Hash size={11} />{value}</span>) : <em>暂无标签</em>}</div>
      <footer>
        <span>{formatDate(component.modified_at)}</span>
        <div><button title="打开目录" onClick={() => void openDirectory()}><FolderOpen size={16} /></button><button title="用 VS Code 打开" onClick={() => void openCode()}><Code2 size={16} /></button><button title="配置组件" onClick={onOpen}><Settings2 size={16} /></button></div>
      </footer>
    </article>
  );
}

function CreateDialog({ onClose, onDone }: DialogProps) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<ComponentKind>("addon");
  const [destination, setDestination] = useState("");
  const [mcs, setMcs] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError("");
    try { const result = await api.create({ name, kind, destination, mcs_compatible: mcs }); onDone(`已创建到 ${result.actual_path}`); }
    catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };
  return <Modal title="新建组件" subtitle="从本地参数化模板生成干净项目" onClose={onClose}><form onSubmit={submit} className="dialog-form"><Field label="组件名称"><input required autoFocus value={name} onChange={(event) => setName(event.target.value)} placeholder="例如：冒险工具包" /></Field><Field label="组件类型"><div className="kind-picker">{(["addon", "material", "map"] as ComponentKind[]).map((value) => <button type="button" className={kind === value ? "selected" : ""} key={value} onClick={() => setKind(value)}>{value === "addon" ? <Box /> : value === "material" ? <Palette /> : <Map />}<span>{kindText[value]}</span></button>)}</div></Field><PathField label="生成位置" value={destination} onChange={setDestination} /><CheckRow checked={mcs} onChange={setMcs} label="生成 MCS 兼容配置" hint="仅当目标是 MCStudio 对应分类目录时启用" />{error && <FormError>{error}</FormError>}<DialogActions busy={busy} onClose={onClose} submit="创建组件" /></form></Modal>;
}

function ImportDialog({ onClose, onDone }: DialogProps) {
  const [source, setSource] = useState("");
  const [destination, setDestination] = useState("");
  const [mcs, setMcs] = useState(false);
  const [policy, setPolicy] = useState<IdentityPolicy>("error");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const chooseSource = async (directory: boolean) => {
    const chosen = await open(directory ? { directory: true, multiple: false } : { multiple: false, filters: [{ name: "组件包", extensions: ["zip", "mcpack", "mcaddon"] }] });
    if (typeof chosen === "string") setSource(chosen);
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError("");
    try { const result = await api.import({ source, destination, mcs_compatible: mcs, identity_policy: policy }); onDone(`已导入到 ${result.actual_path}`); }
    catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };
  return <Modal title="导入组件" subtitle="支持文件夹、ZIP、mcpack 和 mcaddon" onClose={onClose}><form onSubmit={submit} className="dialog-form"><Field label="导入来源"><div className="path-row"><input required value={source} onChange={(event) => setSource(event.target.value)} placeholder="选择组件包或文件夹" /><button type="button" onClick={() => void chooseSource(false)}>选文件</button><button type="button" onClick={() => void chooseSource(true)}>选文件夹</button></div></Field><PathField label="存放位置" value={destination} onChange={setDestination} /><Field label="遇到重复 UUID"><select value={policy} onChange={(event) => setPolicy(event.target.value as IdentityPolicy)}><option value="error">停止并提示</option><option value="regenerate">生成全新 UUID</option><option value="preserve">保留原 UUID</option></select></Field><CheckRow checked={mcs} onChange={setMcs} label="导入为 MCS 组件" hint="将生成新的 MCS UID 与兼容配置" />{error && <FormError>{error}</FormError>}<DialogActions busy={busy} onClose={onClose} submit="开始导入" /></form></Modal>;
}

function SourcesDialog({ onClose, onChanged, onNotice }: { onClose: () => void; onChanged: () => void; onNotice: (message: string) => void }) {
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [vscode, setVscode] = useState<{ available: boolean; path?: string; custom: boolean } | null>(null);
  const load = useCallback(async () => { try { const [nextSources, nextVsCode] = await Promise.all([api.sources(), api.vscodeStatus()]); setSources(nextSources); setVscode(nextVsCode); } catch (error) { onNotice(errorMessage(error)); } }, [onNotice]);
  useEffect(() => { void load(); }, [load]);
  const add = async (library: boolean) => {
    const path = await open({ directory: true, multiple: false });
    if (typeof path !== "string") return;
    try { if (library) await api.addLibrary(path); else await api.addSingle(path); await load(); onChanged(); }
    catch (error) { onNotice(errorMessage(error)); }
  };
  const remove = async (id: string) => { try { await api.removeSource(id); await load(); onChanged(); } catch (error) { onNotice(errorMessage(error)); } };
  const chooseVsCode = async () => { const path = await open({ multiple: false, filters: [{ name: "Visual Studio Code", extensions: ["exe"] }] }); if (typeof path !== "string") return; try { await api.setVsCodePath(path); await load(); } catch (error) { onNotice(errorMessage(error)); } };
  const clearVsCode = async () => { try { await api.setVsCodePath(); await load(); } catch (error) { onNotice(errorMessage(error)); } };
  const copyMcpConfig = async () => { try { await navigator.clipboard.writeText(await api.mcpClientConfig()); onNotice("MCP 客户端配置已复制。" ); } catch (error) { onNotice(errorMessage(error)); } };
  return <Modal title="路径管理" subtitle="移除来源不会删除磁盘上的组件" onClose={onClose} wide><div className="source-actions"><button className="button secondary" onClick={() => void add(true)}><Folder size={17} />添加组件库</button><button className="button secondary" onClick={() => void add(false)}><Box size={17} />添加单个组件</button></div><div className="source-list"><div className="source-row automatic"><span><Sparkles size={17} /></span><div><strong>自动扫描 MCStudio</strong><p>所有逻辑磁盘 · MCStudioDownload\work</p></div><em>内置</em></div>{sources.map((source) => <div className="source-row" key={source.id}><span>{source.kind === "library" ? <Folder size={17} /> : <Box size={17} />}</span><div><strong>{source.kind === "library" ? "组件库" : "单个组件"}</strong><p title={source.path}>{source.path}</p></div><button aria-label="移除来源" onClick={() => void remove(source.id)}><X size={16} /></button></div>)}{!sources.length && <p className="source-empty">尚未添加自定义来源。</p>}</div><div className="settings-divider"><div><strong>Visual Studio Code</strong><p>{vscode?.available ? vscode.path : "未检测到 Code.exe，可手动指定。"}</p></div><button className="button secondary" onClick={() => void chooseVsCode()}>选择程序</button>{vscode?.custom && <button className="button secondary" onClick={() => void clearVsCode()}>恢复自动检测</button>}</div><div className="settings-divider"><div><strong>AI / MCP</strong><p>独立 stdio 服务，不监听网络端口。</p></div><button className="button secondary" onClick={() => void copyMcpConfig()}>复制客户端配置</button></div></Modal>;
}

function ComponentDialog({ component, onClose, onDone, onNotice }: { component: ComponentSummary; onClose: () => void; onDone: (message: string) => void; onNotice: (message: string) => void }) {
  const [tags, setTags] = useState(component.tags.join(", "));
  const [destination, setDestination] = useState("");
  const [mcs, setMcs] = useState(false);
  const [identity, setIdentity] = useState<"preserve" | "regenerate">("regenerate");
  const [part, setPart] = useState<VersionPart>("patch");
  const [busy, setBusy] = useState("");
  const run = async (label: string, action: () => Promise<unknown>, message: string) => { setBusy(label); try { await action(); onDone(message); } catch (error) { onNotice(errorMessage(error)); } finally { setBusy(""); } };
  const needDestination = () => { if (destination) return true; onNotice("请先选择目标目录。" ); return false; };
  return <Modal title={component.name} subtitle={component.path} onClose={onClose} wide><div className="component-dialog"><section><h3>快捷配置</h3><Field label="标签（使用逗号分隔）"><div className="inline-action"><input value={tags} onChange={(event) => setTags(event.target.value)} placeholder="开发, 测试" /><button disabled={!!busy} onClick={() => void run("tags", () => api.tags(component.id, tags.split(/[,，]/)), "标签已保存")}>保存</button></div></Field><div className="config-row"><div><strong>Manifest UUID</strong><p>重生 header、module，并同步内部依赖和地图清单。</p></div><button disabled={!!busy} onClick={() => { if (window.confirm("确定重生所有已识别 manifest UUID？")) void run("uuid", () => api.regenerateUuids(component.id), "UUID 已重新生成"); }}>随机重生</button></div><div className="config-row"><div><strong>包版本</strong><p>同步 header、module、依赖和地图包清单。</p></div><select value={part} onChange={(event) => setPart(event.target.value as VersionPart)}><option value="patch">Patch</option><option value="minor">Minor</option><option value="major">Major</option></select><button disabled={!!busy} onClick={() => void run("version", () => api.bumpVersion(component.id, part), "版本已提升")}>提升版本</button></div></section><section><h3>复制、移动与导出</h3><PathField label="目标目录" value={destination} onChange={setDestination} /><div className="transfer-options"><label>复制 UUID <select value={identity} onChange={(event) => setIdentity(event.target.value as "preserve" | "regenerate")}><option value="regenerate">生成新的</option><option value="preserve">保留</option></select></label><CheckRow checked={mcs} onChange={setMcs} label="目标为 MCS 分类目录" /></div><div className="transfer-buttons"><button className="button secondary" disabled={!!busy} onClick={() => needDestination() && void run("copy", () => api.copy({ component_id: component.id, destination, mcs_compatible: mcs, identity_policy: identity }), "组件已复制")}><Copy size={16} />复制</button><button className="button secondary" disabled={!!busy} onClick={() => needDestination() && window.confirm("移动完成后原目录将被移除，是否继续？") && void run("move", () => api.move({ component_id: component.id, destination, mcs_compatible: mcs }), "组件已移动")}><FolderCog size={16} />移动</button><button className="button secondary" disabled={!!busy} onClick={() => needDestination() && void run("export", () => api.export({ component_id: component.id, destination }), "ZIP 已导出")}><Archive size={16} />导出 ZIP</button></div></section></div></Modal>;
}

function Modal({ title, subtitle, onClose, children, wide = false }: { title: string; subtitle?: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><section className={wide ? "modal wide" : "modal"} role="dialog" aria-modal="true" aria-labelledby="modal-title"><header><div><h2 id="modal-title">{title}</h2>{subtitle && <p title={subtitle}>{subtitle}</p>}</div><button aria-label="关闭" onClick={onClose}><X size={19} /></button></header><div className="modal-content">{children}</div></section></div>;
}

function Field({ label, children }: { label: string; children: ReactNode }) { return <label className="field"><span>{label}</span>{children}</label>; }
function PathField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) { const choose = async () => { const path = await open({ directory: true, multiple: false }); if (typeof path === "string") onChange(path); }; return <Field label={label}><div className="path-row"><input required value={value} onChange={(event) => onChange(event.target.value)} placeholder="选择一个可写目录" /><button type="button" onClick={() => void choose()}>浏览</button></div></Field>; }
function CheckRow({ checked, onChange, label, hint }: { checked: boolean; onChange: (value: boolean) => void; label: string; hint?: string }) { return <label className="check-row"><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /><span><strong>{label}</strong>{hint && <small>{hint}</small>}</span></label>; }
function FormError({ children }: { children: ReactNode }) { return <p className="form-error">{children}</p>; }
function DialogActions({ busy, onClose, submit }: { busy: boolean; onClose: () => void; submit: string }) { return <div className="dialog-actions"><button type="button" className="button secondary" onClick={onClose}>取消</button><button type="submit" className="button primary" disabled={busy}>{busy ? "处理中…" : submit}</button></div>; }
interface DialogProps { onClose: () => void; onDone: (message: string) => void; }
function countKind(components: ComponentSummary[], kind: ComponentKind) { return components.filter((component) => component.kind === kind).length; }
function originText(component: ComponentSummary) { if (component.mcs) return "MCStudio"; if (component.origin.kind === "single") return "单独路径"; return "组件库"; }
function formatDate(value?: string) { if (!value) return "时间未知"; return new Intl.DateTimeFormat("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value)); }
