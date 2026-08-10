import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Archive,
  ArrowDownUp,
  Boxes,
  Box,
  ChevronRight,
  Code2,
  Copy,
  Eye,
  EyeOff,
  ExternalLink,
  Folder,
  FolderCog,
  FolderOpen,
  Hash,
  Import,
  Info,
  Map,
  MessageSquare,
  MoreHorizontal,
  PackagePlus,
  Palette,
  RefreshCw,
  Save,
  ScanSearch,
  Search,
  Settings,
  Settings2,
  Sparkles,
  Tag,
  Trash2,
  TriangleAlert,
  UserRound,
  X,
} from "lucide-react";
import { FormEvent, ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { api, desktop, errorMessage } from "./api";
import type {
  AppSettings,
  ComponentKind,
  ComponentSummary,
  DiscoveryResult,
  DiscoveryWarning,
  IdentityPolicy,
  OperationResult,
  SourceRecord,
  UpdateCheckResult,
  VersionPart,
} from "./types";

const EMPTY_RESULT: DiscoveryResult = { components: [], sources: [], warnings: [] };
const IGNORED_WARNINGS_STORAGE_KEY = "mcdh.ignored-discovery-warnings";
const DEFAULT_SETTINGS: AppSettings = {
  developer_nickname: "MCDH",
  developer_account: "mcdh@local.invalid",
  developer_user_id: "0",
  theme: "system",
};
const kindText: Record<ComponentKind, string> = { addon: "模组", material: "材质", map: "地图" };
type SortKey = "updated" | "name" | "modified" | "created" | "size";
type SortDirection = "asc" | "desc";

export function App() {
  const [result, setResult] = useState(EMPTY_RESULT);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | ComponentKind>("all");
  const [tag, setTag] = useState("all");
  const [sortKey, setSortKey] = useState<SortKey>("modified");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [modal, setModal] = useState<"create" | "import" | "settings" | "warnings" | null>(null);
  const [selected, setSelected] = useState<ComponentSummary | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [ignoredWarningKeys, setIgnoredWarningKeys] = useState<Set<string>>(readIgnoredWarningKeys);

  const refresh = useCallback(async () => {
    if (!desktop) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const next = await api.refresh();
      setResult(next);
      setIgnoredWarningKeys((current) => {
        const active = new Set(next.warnings.map(warningKey));
        const pruned = new Set([...current].filter((key) => active.has(key)));
        if (setsEqual(current, pruned)) return current;
        writeIgnoredWarningKeys(pruned);
        return pruned;
      });
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!desktop) return;
    void api.settings().then(setSettings).catch((error) => setNotice(errorMessage(error)));
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
  }, [settings.theme]);

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
    const filtered = result.components.filter((component) => {
      if (kind !== "all" && component.kind !== kind) return false;
      if (tag !== "all" && !component.tags.includes(tag)) return false;
      return !needle || `${component.name}\n${component.path}`.toLocaleLowerCase().includes(needle);
    });
    const direction = sortDirection === "asc" ? 1 : -1;
    return filtered.sort((left, right) => {
      const compared = compareComponents(left, right, sortKey);
      return compared * direction || left.name.localeCompare(right.name, "zh-CN");
    });
  }, [kind, query, result.components, sortDirection, sortKey, tag]);
  const visibleWarnings = useMemo(
    () => result.warnings.filter((warning) => !ignoredWarningKeys.has(warningKey(warning))),
    [ignoredWarningKeys, result.warnings],
  );
  const ignoredWarningCount = result.warnings.length - visibleWarnings.length;

  const setWarningIgnored = (warning: DiscoveryWarning, ignored: boolean) => {
    setIgnoredWarningKeys((current) => {
      const next = new Set(current);
      if (ignored) next.add(warningKey(warning));
      else next.delete(warningKey(warning));
      writeIgnoredWarningKeys(next);
      return next;
    });
  };

  const removeWarningSource = async (source: SourceRecord) => {
    if (!window.confirm(`确定从 MCDH 中移除来源？\n${source.path}\n磁盘文件不会被删除。`)) return;
    await api.removeSource(source.id);
    if (settings.default_destination === source.path) {
      const saved = await api.setSettings({ ...settings, default_destination: undefined });
      setSettings(saved);
    }
    setNotice("来源记录已移除，磁盘文件未删除。");
    await refresh();
  };

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
        <button className="sidebar-action" onClick={() => setModal("settings")}>
          <Settings size={18} /><span>设置</span><ChevronRight size={15} />
        </button>
        <div className="offline-badge"><span /> 本地优先 · 无自动联网</div>
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
          <label className="select-box sort-box"><ArrowDownUp size={16} /><select aria-label="排序字段" value={sortKey} onChange={(event) => { const value = event.target.value as SortKey; setSortKey(value); setSortDirection(value === "name" ? "asc" : "desc"); }}><option value="updated">MCS 时间</option><option value="name">名称</option><option value="modified">修改日期</option><option value="created">创建日期</option><option value="size">大小</option></select></label>
          <button className="icon-button" aria-label="切换排序方向" title={sortDirection === "desc" ? "当前倒序" : "当前正序"} onClick={() => setSortDirection((value) => value === "desc" ? "asc" : "desc")}><span className={sortDirection === "desc" ? "sort-direction desc" : "sort-direction"}>↑</span></button>
          <button className="icon-button" aria-label="刷新组件" title="刷新组件" onClick={() => void refresh()}><RefreshCw size={17} className={loading ? "spin" : ""} /></button>
          {ignoredWarningCount > 0 && <button className="ignored-warning-button" onClick={() => setModal("warnings")}><EyeOff size={14} />已忽略 {ignoredWarningCount} 个问题</button>}
          <span className="result-count">{components.length} 个组件</span>
        </section>

        {visibleWarnings.length > 0 && <button className="warning-line" onClick={() => setModal("warnings")}><TriangleAlert size={17} /><span>有 {visibleWarnings.length} 个扫描问题，点击查看具体原因并处理。</span><ChevronRight size={16} /></button>}

        <section className="component-grid" aria-live="polite">
          {components.map((component) => <ComponentCard key={component.id} component={component} onOpen={() => setSelected(component)} onNotice={setNotice} />)}
          {!loading && components.length === 0 && (
            <div className="empty-state">
              <span><Boxes size={28} /></span>
              <h2>{result.components.length ? "没有匹配的组件" : "还没有发现组件"}</h2>
              <p>{result.components.length ? "调整搜索词、分类或标签筛选。" : "添加组件库、单个组件，或创建一个新项目。"}</p>
              {!result.components.length && <button className="button secondary" onClick={() => setModal("settings")}><FolderOpen size={17} />配置存放路径</button>}
            </div>
          )}
        </section>
      </main>

      {modal === "create" && <CreateDialog sources={result.sources} settings={settings} onConfigurePaths={() => setModal("settings")} onClose={() => setModal(null)} onDone={(message) => { setModal(null); void done(message); }} />}
      {modal === "import" && <ImportDialog onClose={() => setModal(null)} onDone={(message) => { setModal(null); void done(message); }} />}
      {modal === "settings" && <SettingsDialog settings={settings} onSettings={setSettings} onClose={() => setModal(null)} onChanged={() => void refresh()} onNotice={setNotice} />}
      {modal === "warnings" && <WarningsDialog warnings={result.warnings} sources={result.sources} ignoredKeys={ignoredWarningKeys} onIgnore={setWarningIgnored} onRemoveSource={removeWarningSource} onClose={() => setModal(null)} onNotice={setNotice} />}
      {selected && <ComponentDialog component={selected} onClose={() => setSelected(null)} onDone={(message, operation, refreshAfter) => {
        setSelected(null);
        const updated = operation.component;
        if (!refreshAfter) {
          if (updated) {
            setResult((current) => ({
              ...current,
              components: current.components.map((component) => component.id === updated.id ? updated : component),
            }));
          }
          setNotice(message);
        } else {
          void done(message);
        }
      }} onNotice={setNotice} />}
      {notice && <div className="toast" role="status">{notice}</div>}
    </div>
  );
}

function NavItem({ active, icon, label, count, onClick }: { active: boolean; icon: ReactNode; label: string; count: number; onClick: () => void }) {
  return <button className={active ? "nav-item active" : "nav-item"} onClick={onClick}><span>{icon}</span>{label}<small>{count}</small></button>;
}

function WarningsDialog({ warnings, sources, ignoredKeys, onIgnore, onRemoveSource, onClose, onNotice }: {
  warnings: DiscoveryWarning[];
  sources: SourceRecord[];
  ignoredKeys: Set<string>;
  onIgnore: (warning: DiscoveryWarning, ignored: boolean) => void;
  onRemoveSource: (source: SourceRecord) => Promise<void>;
  onClose: () => void;
  onNotice: (message: string) => void;
}) {
  const [busy, setBusy] = useState("");
  const openDirectory = async (warning: DiscoveryWarning) => {
    setBusy(`open:${warningKey(warning)}`);
    try {
      await api.openWarningDirectory(warning.path);
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setBusy("");
    }
  };
  const removeSource = async (source: SourceRecord) => {
    setBusy(`remove:${source.id}`);
    try {
      await onRemoveSource(source);
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setBusy("");
    }
  };
  return (
    <Modal title="扫描问题" subtitle="查看无法读取的路径并直接处理" onClose={onClose} wide>
      <div className="warnings-dialog">
        <div className="warnings-help"><TriangleAlert size={18} /><p>移除来源只会删除 MCDH 中的路径记录，不会删除磁盘文件。忽略状态保存在本机，可随时重新显示。</p></div>
        {warnings.length ? <div className="warning-list">{warnings.map((warning) => {
          const key = warningKey(warning);
          const source = sourceForWarning(warning, sources);
          const ignored = ignoredKeys.has(key);
          return (
            <article className={ignored ? "warning-item ignored" : "warning-item"} key={key}>
              <div className="warning-item-heading"><div><strong>{warning.message}</strong><p title={warning.path}>{warning.path || "未知路径"}</p></div>{ignored && <span><EyeOff size={12} />已忽略</span>}</div>
              <div className="warning-source">{source ? `${sourceText(source)}：${source.path}` : "未关联到可移除的来源记录"}</div>
              <div className="warning-actions">
                <button className="button secondary" disabled={!!busy || !warning.path} onClick={() => void openDirectory(warning)}><FolderOpen size={15} />打开文件夹</button>
                <button className="button danger" disabled={!!busy || !source} title={source ? "只移除 MCDH 来源记录" : "该问题未关联到来源记录"} onClick={() => source && void removeSource(source)}><Trash2 size={15} />移除来源</button>
                <button className="button secondary" disabled={!!busy} onClick={() => onIgnore(warning, !ignored)}>{ignored ? <Eye size={15} /> : <EyeOff size={15} />}{ignored ? "重新显示" : "忽略"}</button>
              </div>
            </article>
          );
        })}</div> : <div className="warnings-empty"><span><Eye size={22} /></span><strong>当前没有扫描问题</strong><p>刷新组件后，新问题会继续显示在主界面。</p></div>}
        <div className="dialog-actions"><button className="button primary" onClick={onClose}>完成</button></div>
      </div>
    </Modal>
  );
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
        <span>{formatDate(component.modified_at)} · {formatBytes(component.size_bytes)}</span>
        <div><button title="打开目录" onClick={() => void openDirectory()}><FolderOpen size={16} /></button><button title="用 VS Code 打开" onClick={() => void openCode()}><Code2 size={16} /></button><button title="配置组件" onClick={onOpen}><Settings2 size={16} /></button></div>
      </footer>
    </article>
  );
}

function CreateDialog({ sources, settings, onConfigurePaths, onClose, onDone }: DialogProps & { sources: SourceRecord[]; settings: AppSettings; onConfigurePaths: () => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<ComponentKind>("addon");
  const [destination, setDestination] = useState("");
  const [mcs, setMcs] = useState(false);
  const [namespace, setNamespace] = useState("mcdh");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const destinations = useMemo(
    () => sources.filter((source) => mcs ? source.kind === "mcs_auto" && mcsPathMatchesKind(source.path, kind) : source.kind === "library"),
    [kind, mcs, sources],
  );
  useEffect(() => {
    setDestination((current) => {
      if (destinations.some((source) => source.path === current)) return current;
      const preferred = destinations.find((source) => source.path === settings.default_destination);
      return preferred?.path ?? destinations[0]?.path ?? "";
    });
  }, [destinations, settings.default_destination]);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError("");
    try { const result = await api.create({ name, kind, destination, mcs_compatible: mcs, namespace: mcs ? namespace : undefined }); onDone(`已创建到 ${result.actual_path}`); }
    catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };
  return (
    <Modal title="新建组件" subtitle="从本地参数化模板生成干净项目" onClose={onClose}>
      <form onSubmit={submit} className="dialog-form">
        <Field label="组件名称"><input required autoFocus value={name} onChange={(event) => setName(event.target.value)} placeholder="例如：冒险工具包" /></Field>
        <Field label="组件类型"><div className="kind-picker">{(["addon", "material", "map"] as ComponentKind[]).map((value) => <button type="button" className={kind === value ? "selected" : ""} key={value} onClick={() => setKind(value)}>{value === "addon" ? <Box /> : value === "material" ? <Palette /> : <Map />}<span>{kindText[value]}</span></button>)}</div></Field>
        <CheckRow checked={mcs} onChange={setMcs} label="生成 MCS 兼容配置" hint="启用后只显示与组件类型匹配的 MCStudio 分类目录" />
        {mcs && <Field label="命名空间"><input required pattern="[a-z][a-z0-9_]{0,63}" value={namespace} onChange={(event) => setNamespace(event.target.value)} placeholder="mcdh" /></Field>}
        <Field label="生成位置">{destinations.length ? <select required value={destination} onChange={(event) => setDestination(event.target.value)}>{destinations.map((source) => <option key={source.id} value={source.path}>{destinationLabel(source)}</option>)}</select> : <div className="empty-destination"><span>{mcs ? "尚未配置匹配的 MCS 作品目录。" : "尚未配置组件库目录。"}</span><button type="button" className="button secondary" onClick={onConfigurePaths}>前往设置</button></div>}</Field>
        {error && <FormError>{error}</FormError>}
        <DialogActions busy={busy} disabled={!destination} onClose={onClose} submit="创建组件" />
      </form>
    </Modal>
  );
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

type SettingsSection = "paths" | "identity" | "appearance" | "tools" | "about";

const FEEDBACK_URL = "https://github.com/xiaobo121388/MCDevHelper/issues/new";

function SettingsDialog({ settings: initialSettings, onSettings, onClose, onChanged, onNotice }: { settings: AppSettings; onSettings: (settings: AppSettings) => void; onClose: () => void; onChanged: () => void; onNotice: (message: string) => void }) {
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [settings, setSettings] = useState(initialSettings);
  const [vscode, setVscode] = useState<{ available: boolean; path?: string; custom: boolean } | null>(null);
  const [section, setSection] = useState<SettingsSection>("paths");
  const [appVersion, setAppVersion] = useState("");
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
  const [updateError, setUpdateError] = useState("");
  const [busy, setBusy] = useState("");
  const load = useCallback(async () => {
    try {
      const [nextSources, nextVsCode, nextSettings, nextVersion] = await Promise.all([
        api.sources(), api.vscodeStatus(), api.settings(), api.version(),
      ]);
      setSources(nextSources);
      setVscode(nextVsCode);
      setSettings(nextSettings);
      setAppVersion(nextVersion);
      onSettings(nextSettings);
    } catch (error) {
      onNotice(errorMessage(error));
    }
  }, [onNotice, onSettings]);
  useEffect(() => { void load(); }, [load]);
  const add = async (kind: "library" | "single" | "mcs") => {
    const path = await open({ directory: true, multiple: false });
    if (typeof path !== "string") return;
    setBusy(kind);
    try {
      if (kind === "library") await api.addLibrary(path);
      else if (kind === "single") await api.addSingle(path);
      else await api.addMcsPath(path);
      await load();
      onChanged();
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setBusy("");
    }
  };
  const rescanMcs = async () => {
    setBusy("scan");
    try {
      const found = await api.rescanMcsPaths();
      await load();
      onChanged();
      onNotice(`已保存 ${found.length} 个 MCS 分类目录。`);
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setBusy("");
    }
  };
  const remove = async (source: SourceRecord) => {
    try {
      await api.removeSource(source.id);
      if (settings.default_destination === source.path) {
        const saved = await api.setSettings({ ...settings, default_destination: undefined });
        setSettings(saved);
        onSettings(saved);
      }
      await load();
      onChanged();
    } catch (error) {
      onNotice(errorMessage(error));
    }
  };
  const save = async () => {
    setBusy("save");
    try {
      const saved = await api.setSettings(settings);
      setSettings(saved);
      onSettings(saved);
      onNotice("设置已保存。");
    } catch (error) {
      onNotice(errorMessage(error));
    } finally {
      setBusy("");
    }
  };
  const chooseVsCode = async () => { const path = await open({ multiple: false, filters: [{ name: "Visual Studio Code", extensions: ["exe"] }] }); if (typeof path !== "string") return; try { await api.setVsCodePath(path); await load(); } catch (error) { onNotice(errorMessage(error)); } };
  const clearVsCode = async () => { try { await api.setVsCodePath(); await load(); } catch (error) { onNotice(errorMessage(error)); } };
  const copyMcpConfig = async () => { try { await navigator.clipboard.writeText(await api.mcpClientConfig()); onNotice("MCP 客户端配置已复制。" ); } catch (error) { onNotice(errorMessage(error)); } };
  const checkForUpdates = async () => {
    setBusy("update");
    setUpdateError("");
    try {
      setUpdateResult(await api.checkForUpdates());
    } catch (error) {
      setUpdateResult(null);
      setUpdateError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };
  const openGitHub = async (url: string) => {
    if (!url.startsWith("https://github.com/xiaobo121388/MCDevHelper/")) {
      onNotice("拒绝打开非官方 MCDH 链接。");
      return;
    }
    try {
      await openUrl(url);
    } catch (error) {
      onNotice(errorMessage(error));
    }
  };
  const close = () => { document.documentElement.dataset.theme = initialSettings.theme; onClose(); };
  const destinationSources = sources.filter((source) => source.kind !== "single");
  const settingsNavigation: { id: SettingsSection; label: string; icon: ReactNode }[] = [
    { id: "paths", label: "路径管理", icon: <FolderCog size={17} /> },
    { id: "identity", label: "MCS 身份", icon: <UserRound size={17} /> },
    { id: "appearance", label: "外观", icon: <Palette size={17} /> },
    { id: "tools", label: "开发工具", icon: <Code2 size={17} /> },
    { id: "about", label: "关于", icon: <Info size={17} /> },
  ];
  return (
    <Modal title="设置" subtitle="按类别管理路径、身份、外观与开发工具" onClose={close} wide className="settings-modal">
      <div className="settings-layout">
        <nav className="settings-nav" aria-label="设置分类">
          {settingsNavigation.map((item) => (
            <button
              key={item.id}
              className={section === item.id ? "active" : ""}
              aria-current={section === item.id ? "page" : undefined}
              onClick={() => setSection(item.id)}
            >
              {item.icon}<span>{item.label}</span><ChevronRight size={14} />
            </button>
          ))}
        </nav>

        <div className="settings-panel">
          <div className="settings-panel-body">
            {section === "paths" && <section>
              <div className="section-heading"><div><h3>路径管理</h3><p>首次无记录时自动发现 MCS；之后只使用这里保存的目录。</p></div><button className="button secondary" disabled={!!busy} onClick={() => void rescanMcs()}><ScanSearch size={16} />重新扫描 MCS</button></div>
              <div className="source-actions"><button className="button secondary" disabled={!!busy} onClick={() => void add("library")}><Folder size={17} />添加组件库</button><button className="button secondary" disabled={!!busy} onClick={() => void add("single")}><Box size={17} />添加单个组件</button><button className="button secondary" disabled={!!busy} onClick={() => void add("mcs")}><Sparkles size={17} />添加 MCS 路径</button></div>
              <div className="source-list">{sources.map((source) => <div className="source-row" key={source.id}><span>{source.kind === "mcs_auto" ? <Sparkles size={17} /> : source.kind === "library" ? <Folder size={17} /> : <Box size={17} />}</span><div><strong>{sourceText(source)}</strong><p title={source.path}>{source.path}</p></div><button aria-label={`移除 ${source.path}`} onClick={() => void remove(source)}><X size={16} /></button></div>)}{!sources.length && <p className="source-empty">尚未保存来源目录。</p>}</div>
              <Field label="新建组件默认位置"><select value={settings.default_destination ?? ""} onChange={(event) => setSettings((value) => ({ ...value, default_destination: event.target.value || undefined }))}><option value="">不指定（使用可用列表第一项）</option>{destinationSources.map((source) => <option key={source.id} value={source.path}>{destinationLabel(source)}</option>)}</select></Field>
            </section>}

            {section === "identity" && <section>
              <div className="section-heading"><div><h3>MCStudio 开发者身份</h3><p>这些信息只用于生成本地 MCS 兼容文件，不用于登录。</p></div><UserRound size={19} /></div>
              <div className="settings-grid"><Field label="开发者昵称"><input value={settings.developer_nickname} onChange={(event) => setSettings((value) => ({ ...value, developer_nickname: event.target.value }))} /></Field><Field label="开发者账号"><input value={settings.developer_account} onChange={(event) => setSettings((value) => ({ ...value, developer_account: event.target.value }))} /></Field><Field label="用户 ID"><input value={settings.developer_user_id} onChange={(event) => setSettings((value) => ({ ...value, developer_user_id: event.target.value }))} /></Field></div>
            </section>}

            {section === "appearance" && <section>
              <div className="section-heading"><div><h3>外观</h3><p>选择亮色、暗色，或跟随 Windows 系统设置。</p></div><Palette size={19} /></div>
              <div className="settings-grid single"><Field label="界面颜色"><select value={settings.theme} onChange={(event) => { const theme = event.target.value as AppSettings["theme"]; setSettings((value) => ({ ...value, theme })); document.documentElement.dataset.theme = theme; }}><option value="system">跟随系统</option><option value="light">亮色</option><option value="dark">暗色</option></select></Field></div>
            </section>}

            {section === "tools" && <section>
              <div className="section-heading"><div><h3>开发工具</h3><p>配置编辑器，以及供 AI 使用的本地 MCP 服务。</p></div><Code2 size={19} /></div>
              <div className="settings-tool"><div><strong>Visual Studio Code</strong><p>{vscode?.available ? vscode.path : "未检测到 Code.exe，可手动指定。"}</p></div><div className="settings-tool-actions"><button className="button secondary" onClick={() => void chooseVsCode()}>选择程序</button>{vscode?.custom && <button className="button secondary" onClick={() => void clearVsCode()}>恢复自动检测</button>}</div></div>
              <div className="settings-tool"><div><strong>AI / MCP</strong><p>独立 stdio 服务，只读写本机组件，不监听网络端口。</p></div><div className="settings-tool-actions"><button className="button secondary" onClick={() => void copyMcpConfig()}>复制客户端配置</button></div></div>
            </section>}

            {section === "about" && <section>
              <div className="about-product"><span className="brand-mark"><Boxes size={21} /></span><div><h3>MCDH · MCDevHelper</h3><p>网易中国版 PE 创作者的本地组件管理工具</p></div><strong>v{appVersion || "…"}</strong></div>
              <div className="about-note"><Info size={17} /><p>默认不会联网。只有主动点击“检查更新”时才会访问 GitHub Releases API；反馈会交给系统浏览器打开 GitHub Issue 页面。</p></div>
              <div className="settings-tool"><div><strong>检查更新</strong><p>查询 GitHub 上最新发布的正式 Release，不会自动下载或安装。</p></div><div className="settings-tool-actions"><button className="button primary" disabled={busy === "update"} onClick={() => void checkForUpdates()}><RefreshCw className={busy === "update" ? "spin" : ""} size={16} />{busy === "update" ? "检查中…" : "检查更新"}</button></div></div>
              {updateError && <div className="update-result error"><TriangleAlert size={17} /><div><strong>检查失败</strong><p>{updateError}</p></div></div>}
              {updateResult && <div className={`update-result ${updateResult.update_available ? "available" : "current"}`}>
                <Info size={17} />
                <div>
                  <strong>{updateResult.no_release ? "尚无正式 Release" : updateResult.update_available ? `发现新版本 ${updateResult.latest_version}` : "当前已是最新版本"}</strong>
                  <p>{updateResult.no_release ? "官方仓库目前没有可供检查的正式 Release。" : `${updateResult.release_name || updateResult.latest_version}${updateResult.published_at ? ` · ${formatDate(updateResult.published_at)}` : ""}`}</p>
                </div>
                {updateResult.release_url && <button className="button secondary" onClick={() => void openGitHub(updateResult.release_url!)}>查看 Release<ExternalLink size={14} /></button>}
              </div>}
              <div className="settings-tool"><div><strong>反馈问题</strong><p>在 GitHub 新建 Issue；提交内容前由你自行确认。</p></div><div className="settings-tool-actions"><button className="button secondary" onClick={() => void openGitHub(FEEDBACK_URL)}><MessageSquare size={16} />打开反馈页面<ExternalLink size={14} /></button></div></div>
            </section>}
          </div>
          <div className="settings-footer"><button className="button secondary" onClick={close}>关闭</button><button className="button primary" disabled={!!busy} onClick={() => void save()}><Save size={16} />{busy === "save" ? "保存中…" : "保存设置"}</button></div>
        </div>
      </div>
    </Modal>
  );
}

function ComponentDialog({ component, onClose, onDone, onNotice }: { component: ComponentSummary; onClose: () => void; onDone: (message: string, operation: OperationResult, refreshAfter: boolean) => void; onNotice: (message: string) => void }) {
  const [tags, setTags] = useState(component.tags.join(", "));
  const [destination, setDestination] = useState("");
  const [mcs, setMcs] = useState(false);
  const [identity, setIdentity] = useState<"preserve" | "regenerate">("regenerate");
  const [part, setPart] = useState<VersionPart>("patch");
  const [busy, setBusy] = useState("");
  const run = async (label: string, action: () => Promise<OperationResult>, message: string, refreshAfter = true) => { setBusy(label); try { onDone(message, await action(), refreshAfter); } catch (error) { onNotice(errorMessage(error)); } finally { setBusy(""); } };
  const needDestination = () => { if (destination) return true; onNotice("请先选择目标目录。" ); return false; };
  const remove = () => {
    if (!window.confirm(`确定删除“${component.name}”吗？`)) return;
    if (!window.confirm(`再次确认：将永久删除磁盘目录\n${component.path}\n此操作无法撤销。`)) return;
    void run("delete", () => api.delete(component.id), "组件已删除");
  };
  return (
    <Modal title={component.name} subtitle={component.path} onClose={onClose} wide>
      <div className="component-dialog">
        <section>
          <h3>快捷配置</h3>
          <Field label="标签（使用逗号分隔）"><div className="inline-action"><input value={tags} onChange={(event) => setTags(event.target.value)} placeholder="开发, 测试" /><button disabled={!!busy} onClick={() => void run("tags", () => api.tags(component.id, tags.split(/[,，]/)), "标签已保存")}>保存</button></div></Field>
          <div className="config-row"><div><strong>Manifest UUID</strong><p>重生 header、module，并同步内部依赖和地图清单；保留 JSONC 注释。</p></div><button disabled={!!busy} onClick={() => { if (window.confirm("确定重生所有已识别 manifest UUID？")) void run("uuid", () => api.regenerateUuids(component.id), "UUID 已重新生成", false); }}>随机重生</button></div>
          <div className="config-row"><div><strong>包版本</strong><p>同步 header、module、依赖和地图包清单；保留 JSONC 注释。</p></div><select value={part} onChange={(event) => setPart(event.target.value as VersionPart)}><option value="patch">Patch</option><option value="minor">Minor</option><option value="major">Major</option></select><button disabled={!!busy} onClick={() => void run("version", () => api.bumpVersion(component.id, part), "版本已提升", false)}>提升版本</button></div>
        </section>
        <section>
          <h3>复制、移动、导出与删除</h3>
          <PathField label="目标目录" value={destination} onChange={setDestination} />
          <div className="transfer-options"><label>复制 UUID <select value={identity} onChange={(event) => setIdentity(event.target.value as "preserve" | "regenerate")}><option value="regenerate">生成新的</option><option value="preserve">保留</option></select></label><CheckRow checked={mcs} onChange={setMcs} label="目标为 MCS 分类目录" /></div>
          <div className="transfer-buttons"><button className="button secondary" disabled={!!busy} onClick={() => needDestination() && void run("copy", () => api.copy({ component_id: component.id, destination, mcs_compatible: mcs, identity_policy: identity }), "组件已复制")}><Copy size={16} />复制</button><button className="button secondary" disabled={!!busy} onClick={() => needDestination() && window.confirm("移动完成后原目录将被移除，是否继续？") && void run("move", () => api.move({ component_id: component.id, destination, mcs_compatible: mcs }), "组件已移动")}><FolderCog size={16} />移动</button><button className="button secondary" disabled={!!busy} onClick={() => needDestination() && void run("export", () => api.export({ component_id: component.id, destination }), "ZIP 已导出", false)}><Archive size={16} />{busy === "export" ? "导出中…" : "导出 ZIP"}</button><button className="button danger" disabled={!!busy} onClick={remove}><Trash2 size={16} />删除</button></div>
        </section>
      </div>
    </Modal>
  );
}

function Modal({ title, subtitle, onClose, children, wide = false, className = "" }: { title: string; subtitle?: string; onClose: () => void; children: ReactNode; wide?: boolean; className?: string }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><section className={["modal", wide ? "wide" : "", className].filter(Boolean).join(" ")} role="dialog" aria-modal="true" aria-labelledby="modal-title"><header><div><h2 id="modal-title">{title}</h2>{subtitle && <p title={subtitle}>{subtitle}</p>}</div><button aria-label="关闭" onClick={onClose}><X size={19} /></button></header><div className="modal-content">{children}</div></section></div>;
}

function Field({ label, children }: { label: string; children: ReactNode }) { return <label className="field"><span>{label}</span>{children}</label>; }
function PathField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) { const choose = async () => { const path = await open({ directory: true, multiple: false }); if (typeof path === "string") onChange(path); }; return <Field label={label}><div className="path-row"><input required value={value} onChange={(event) => onChange(event.target.value)} placeholder="选择一个可写目录" /><button type="button" onClick={() => void choose()}>浏览</button></div></Field>; }
function CheckRow({ checked, onChange, label, hint }: { checked: boolean; onChange: (value: boolean) => void; label: string; hint?: string }) { return <label className="check-row"><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /><span><strong>{label}</strong>{hint && <small>{hint}</small>}</span></label>; }
function FormError({ children }: { children: ReactNode }) { return <p className="form-error">{children}</p>; }
function DialogActions({ busy, disabled = false, onClose, submit }: { busy: boolean; disabled?: boolean; onClose: () => void; submit: string }) { return <div className="dialog-actions"><button type="button" className="button secondary" onClick={onClose}>取消</button><button type="submit" className="button primary" disabled={busy || disabled}>{busy ? "处理中…" : submit}</button></div>; }
interface DialogProps { onClose: () => void; onDone: (message: string) => void; }
function countKind(components: ComponentSummary[], kind: ComponentKind) { return components.filter((component) => component.kind === kind).length; }
function originText(component: ComponentSummary) { if (component.mcs) return "MCStudio"; if (component.origin.kind === "single") return "单独路径"; return "组件库"; }
function formatDate(value?: string) { if (!value) return "时间未知"; return new Intl.DateTimeFormat("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value)); }
function formatBytes(value = 0) { if (value < 1024) return `${value} B`; const units = ["KB", "MB", "GB", "TB"]; let size = value / 1024; let unit = units[0]; for (let index = 1; size >= 1024 && index < units.length; index += 1) { size /= 1024; unit = units[index]; } return `${size >= 10 ? size.toFixed(0) : size.toFixed(1)} ${unit}`; }
function compareComponents(left: ComponentSummary, right: ComponentSummary, key: SortKey) {
  if (key === "name") return left.name.localeCompare(right.name, "zh-CN", { numeric: true, sensitivity: "base" });
  if (key === "size") return (left.size_bytes ?? 0) - (right.size_bytes ?? 0);
  const field = key === "updated" ? "updated_at" : key === "created" ? "created_at" : "modified_at";
  return Date.parse(left[field] ?? "1970-01-01") - Date.parse(right[field] ?? "1970-01-01");
}
function mcsPathMatchesKind(path: string, kind: ComponentKind) { const category = path.split(/[\\/]/).filter(Boolean).at(-1)?.toLocaleLowerCase(); if (kind === "addon") return category === "addon"; if (kind === "map") return category === "map"; return category === "material" || category === "light"; }
function sourceText(source: SourceRecord) { if (source.kind === "mcs_auto") return "MCS 作品目录"; if (source.kind === "library") return "组件库"; return "单个组件"; }
function destinationLabel(source: SourceRecord) { const name = source.path.split(/[\\/]/).filter(Boolean).at(-1) ?? source.path; return `${source.kind === "mcs_auto" ? `MCStudio · ${name}` : "组件库"} — ${source.path}`; }
function warningKey(warning: DiscoveryWarning) { return `${normalizeWarningPath(warning.path)}\n${warning.message}`; }
function normalizeWarningPath(path: string) { return path.replace(/\//g, "\\").replace(/\\+$/, "").toLocaleLowerCase(); }
function sourceForWarning(warning: DiscoveryWarning, sources: SourceRecord[]) {
  const warningPath = normalizeWarningPath(warning.path);
  return sources
    .filter((source) => { const sourcePath = normalizeWarningPath(source.path); return warningPath === sourcePath || warningPath.startsWith(`${sourcePath}\\`); })
    .sort((left, right) => right.path.length - left.path.length)[0];
}
function readIgnoredWarningKeys() {
  if (typeof window === "undefined") return new Set<string>();
  try {
    const stored = JSON.parse(window.localStorage.getItem(IGNORED_WARNINGS_STORAGE_KEY) ?? "[]");
    return new Set<string>(Array.isArray(stored) ? stored.filter((value): value is string => typeof value === "string") : []);
  } catch {
    return new Set<string>();
  }
}
function writeIgnoredWarningKeys(keys: Set<string>) {
  if (typeof window === "undefined") return;
  try { window.localStorage.setItem(IGNORED_WARNINGS_STORAGE_KEY, JSON.stringify([...keys].sort())); } catch { /* Storage can be unavailable in restricted WebViews. */ }
}
function setsEqual(left: Set<string>, right: Set<string>) { return left.size === right.size && [...left].every((value) => right.has(value)); }
