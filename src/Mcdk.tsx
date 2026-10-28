import { open } from "@tauri-apps/plugin-dialog";
import { Download, FolderOpen, LoaderCircle, Monitor, Play, RefreshCw, RotateCcw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, desktop, errorMessage } from "./api";
import type { McdkSession, McdkStatus } from "./types";

// The backend host validator requires this branch before reporting host_available.
const MCDE_HOST_BRANCH = "feature/plugin-system";

function mcdeLoaded(status: McdkStatus | null): boolean {
  return !!status?.mcde && status.mcde_plugin_installed !== false && status.mcde_plugin_enabled !== false;
}

function mcdeIssue(status: McdkStatus | null): string | null {
  if (!status) return "正在读取 MCDE 状态";
  if (status.mcde_plugin_enabled === false) return "MCDE 插件已禁用，请在插件页启用";
  if (!status.mcde) return "未检测到 MCDE 插件，请在开发工具中配置插件目录";
  if (status.mcde.error) return status.mcde.error;
  if (!status.mcde.host_available) return "MCDE 宿主不可用";
  if (!status.mcde.available) return "MCDE 插件不可用，请在开发工具中配置插件目录";
  return null;
}

export function useMcdk(onNotice: (message: string) => void) {
  const [status, setStatus] = useState<McdkStatus | null>(null);
  const [launching, setLaunching] = useState<string | null>(null);
  const [launchingMode, setLaunchingMode] = useState<NonNullable<McdkSession["mode"]> | null>(null);
  const [error, setError] = useState("");
  const pending = useRef(false);
  const currentStatus = useRef<McdkStatus | null>(null);
  const lastExit = useRef<string | null | undefined>(undefined);
  const revision = useRef(0);
  const accept = useCallback((next: McdkStatus) => {
    revision.current += 1;
    currentStatus.current = next;
    setStatus(next);
    setError("");
    if (lastExit.current !== undefined && next.last_exit && next.last_exit.id !== lastExit.current
      && next.last_exit.exit_code !== null && next.last_exit.exit_code !== 0
      && (next.last_exit.mode !== "editor" || mcdeLoaded(next))) {
      onNotice(next.last_exit.mode === "editor"
        ? "MCDE 宿主异常退出（代码 " + next.last_exit.exit_code + "），日志目录：" + (next.last_exit.launch_directory ?? "未记录")
        : "MCDK 启动器异常退出（代码 " + next.last_exit.exit_code + "），请查看控制台输出。");
    }
    lastExit.current = next.last_exit?.id ?? null;
  }, [onNotice]);

  const refresh = useCallback(async () => {
    const before = revision.current;
    try {
      const next = await api.mcdkStatus();
      if (revision.current === before) accept(next);
    } catch (cause) { if (revision.current === before) setError(errorMessage(cause)); }
  }, [accept]);

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const stop = await api.onMcdkStatus((next) => { if (active) accept(next); });
        if (!active) { stop(); return; }
        unlisten = stop;
        const before = revision.current;
        const next = await api.mcdkStatus();
        if (active && revision.current === before) accept(next);
      } catch (cause) { if (active) setError(errorMessage(cause)); }
    })();
    return () => { active = false; unlisten?.(); };
  }, [accept]);

  const startLaunch = async (id: string, mode: "game" | "editor") => {
    const current = currentStatus.current;
    if (pending.current || !desktop || !current || current.session
      || (mode === "editor" ? !mcdeLoaded(current) || !!mcdeIssue(current) : !current.available)) return;
    pending.current = true;
    revision.current += 1;
    setLaunching(id);
    setLaunchingMode(mode);
    try {
      const session = await (mode === "editor" ? api.launchEditor(id) : api.launchGame(id));
      if (currentStatus.current) accept({ ...currentStatus.current, session });
      onNotice(mode === "editor" ? "已启动 MCDE 宿主（游戏就绪状态未确认）" : "已打开 MCDK 启动器");
    } catch (cause) { onNotice(errorMessage(cause)); }
    finally { await refresh(); pending.current = false; setLaunching(null); setLaunchingMode(null); }
  };
  const launch = (id: string) => startLaunch(id, "game");
  const launchEditor = (id: string) => startLaunch(id, "editor");

  const update = async (action: () => Promise<McdkStatus>) => {
    const before = revision.current;
    try {
      const next = await action();
      if (revision.current === before) accept(next); else await refresh();
    }
    catch (cause) { await refresh(); setError(errorMessage(cause)); }
  };
  return { status, launching, launchingMode, error, launch, launchEditor, update, refresh };
}

export type McdkController = ReturnType<typeof useMcdk>;

export function LaunchGameButton({ componentId, name, mcdk, disabled = false }: { componentId: string; name: string; mcdk: McdkController; disabled?: boolean }) {
  const session = mcdk.status?.session;
  const running = session?.component_id === componentId;
  const editorLoaded = mcdeLoaded(mcdk.status);
  const editorRunning = editorLoaded && running && session?.mode === "editor";
  const waiting = mcdk.launching === componentId && mcdk.launchingMode === "game";
  const editorWaiting = mcdk.launching === componentId && mcdk.launchingMode === "editor";
  const busy = disabled || !desktop || !!mcdk.launching || !!session;
  const editorTitle = !desktop ? "请在桌面应用中打开 MCDE" : mcdeIssue(mcdk.status)
    ?? (session ? editorRunning ? "MCDE 宿主运行中" : "请先退出正在运行的 MCDK 会话"
      : mcdk.launching ? "正在启动会话" : disabled ? "正在导出，请稍候" : "在 MCDE 中打开 " + name);
  const title = editorRunning ? "MCDE 宿主运行中" : running ? "MCDK 会话运行中" : session ? "请先退出正在运行的 MCDK 会话" : "启动游戏";
  return <>{editorLoaded && <button className={editorRunning ? "launch-editor active" : "launch-editor"} title={editorTitle}
    aria-label={"在 MCDE 中打开 " + name} aria-busy={editorWaiting}
    disabled={busy || !!mcdeIssue(mcdk.status)} onClick={() => void mcdk.launchEditor(componentId)}>
    {editorWaiting ? <LoaderCircle size={16} className="mcdk-spin" /> : <Monitor size={16} />}
  </button>}<button className={running && !editorRunning ? "launch-game active" : "launch-game"} title={title}
    aria-label={"启动游戏 " + name} aria-busy={waiting}
    disabled={busy || !mcdk.status?.available}
    onClick={() => void mcdk.launch(componentId)}>
    {waiting ? <LoaderCircle size={16} className="mcdk-spin" /> : <Play size={16} />}
  </button></>;
}

const phaseText: Record<McdkStatus["phase"], string> = {
  idle: "已是最新正式版", checking: "正在检查更新", downloading: "正在下载",
  available: "发现新版本", updated: "更新完成", error: "更新失败",
};

export function McdkSettings({ mcdk }: { mcdk: McdkController }) {
  const { status } = mcdk;
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const run = async (action: () => Promise<McdkStatus>) => {
    setBusy(true);
    try { await mcdk.update(action); } finally { setBusy(false); }
  };
  const toggle = async (enabled: boolean) => {
    setSaving(true);
    try { await mcdk.update(() => api.setMcdkAutoUpdate(enabled)); } finally { setSaving(false); }
  };
  const working = busy || status?.phase === "checking" || status?.phase === "downloading";
  const percent = status?.download_size ? Math.min(100, Math.floor(status.downloaded_bytes / status.download_size * 100)) : 0;
  return <div className="mcdk-settings">
    <div className="mcdk-heading"><strong>MCDK</strong><span>{status?.current_version ? "v" + status.current_version : "未就绪"}</span></div>
    <div className="mcdk-status" role="status">{status ? (status.last_checked_at ? phaseText[status.phase] : "尚未检查更新") : "正在读取状态"}{status?.latest_version && " · v" + status.latest_version}</div>
    {status?.phase === "downloading" && <progress aria-label="MCDK 下载进度" max={100} value={percent} />}
    {status?.last_checked_at && <p className="mcdk-checked">最近检查：{new Date(status.last_checked_at).toLocaleString()}</p>}
    {status?.session && <p className="mcdk-session">{mcdeLoaded(status) && status.session.mode === "editor" ? "MCDE 宿主运行中 · " + status.session.version : "运行中 · v" + status.session.version} · PID {status.session.pid}</p>}
    {(mcdk.error || status?.error) && <p className="mcdk-error" role="alert">{mcdk.error || status?.error}</p>}
    <div className="mcdk-actions">
      <label className="mcdk-toggle"><input type="checkbox" role="switch" aria-label="MCDK 自动更新" checked={status?.auto_update ?? true} disabled={!status || saving} onChange={(event) => void toggle(event.target.checked)} /><span>自动更新</span></label>
      <div><button className="button secondary" disabled={!desktop || working} onClick={() => void run(api.checkMcdkUpdate)}><RefreshCw size={15} className={status?.phase === "checking" ? "mcdk-spin" : undefined} />检查更新</button>
        {status?.latest_version && <button className="button secondary" disabled={working} onClick={() => void run(api.installMcdkUpdate)}><Download size={15} />立即更新</button>}</div>
    </div>
    <McdeSettings mcdk={mcdk} showError={false} />
  </div>;
}

export function McdeSettings({ mcdk, showError = true }: { mcdk: McdkController; showError?: boolean }) {
  const { status } = mcdk;
  const [configuring, setConfiguring] = useState(false);
  const [directoryError, setDirectoryError] = useState("");
  const directoryPending = useRef(false);
  const configureDirectory = async (reset = false) => {
    if (directoryPending.current || !desktop || !status || status.session || mcdk.launching) return;
    directoryPending.current = true;
    setConfiguring(true);
    setDirectoryError("");
    try {
      let directory: string | null = null;
      if (!reset) {
        const selected = await open({ title: "选择 MCDE 插件目录", directory: true, multiple: false, defaultPath: status.mcde?.directory ?? undefined });
        if (typeof selected !== "string") return;
        directory = selected;
      }
      await mcdk.update(() => api.setMcdeDirectory(directory));
    } catch (cause) { setDirectoryError(errorMessage(cause)); }
    finally { directoryPending.current = false; setConfiguring(false); }
  };
  const directoryDisabled = !desktop || !status || configuring || !!status.session || !!mcdk.launching;
  const editorIssue = status ? mcdeIssue(status) : null;
  const error = directoryError || (showError && mcdk.error) || editorIssue;
  if (!mcdeLoaded(status)) return null;
  return <section className="mcde-settings" aria-label="MCDE">
      <div className="mcdk-heading"><strong>MCDE</strong><span>{status?.mcde?.plugin_version ? "v" + status.mcde.plugin_version : status ? "未检测到插件" : "正在读取状态"}</span></div>
      <label className="field mcde-directory"><span>MCDE 插件目录<small>{status?.mcde?.configured ? "自定义" : "内置默认"}</small></span>
        <div className="path-row"><input aria-label="MCDE 插件目录" readOnly value={status?.mcde?.directory ?? ""} title={status?.mcde?.directory ?? undefined} placeholder="未检测到插件目录" aria-invalid={!!editorIssue} />
          <button type="button" title="选择 MCDE 插件目录" aria-label="选择 MCDE 插件目录" aria-busy={configuring} disabled={directoryDisabled} onClick={() => void configureDirectory()}>{configuring ? <LoaderCircle size={16} className="mcdk-spin" /> : <FolderOpen size={16} />}</button>
          <button type="button" title="恢复内置 MCDE 插件" aria-label="恢复内置 MCDE 插件" disabled={directoryDisabled || !status?.mcde?.configured} onClick={() => void configureDirectory(true)}><RotateCcw size={16} /></button>
        </div>
      </label>
      <p className="mcde-host">宿主分支 / 修订：{status?.mcde?.host_available ? MCDE_HOST_BRANCH + " · " + (status.mcde.host_revision ?? "未提供") : "宿主不可用"}</p>
      {error && <p className="mcdk-error" role="alert">{error}</p>}
    </section>;
}
