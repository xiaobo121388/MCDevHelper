import { Download, LoaderCircle, Play, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, desktop, errorMessage } from "./api";
import type { McdkStatus } from "./types";

export function useMcdk(onNotice: (message: string) => void) {
  const [status, setStatus] = useState<McdkStatus | null>(null);
  const [launching, setLaunching] = useState<string | null>(null);
  const [error, setError] = useState("");
  const pending = useRef(false);
  const lastExit = useRef<string | null | undefined>(undefined);
  const revision = useRef(0);
  const accept = useCallback((next: McdkStatus) => {
    revision.current += 1;
    setStatus(next);
    setError("");
    if (lastExit.current !== undefined && next.last_exit && next.last_exit.id !== lastExit.current
      && next.last_exit.exit_code !== null && next.last_exit.exit_code !== 0) {
      onNotice("MCDK 启动器异常退出（代码 " + next.last_exit.exit_code + "），请查看控制台输出。");
    }
    lastExit.current = next.last_exit?.id ?? null;
  }, [onNotice]);

  const refresh = useCallback(async () => {
    const before = revision.current;
    try {
      const next = await api.mcdkStatus();
      if (revision.current === before) accept(next);
    } catch (cause) { setError(errorMessage(cause)); }
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

  const launch = async (id: string) => {
    if (pending.current || status?.session) return;
    pending.current = true;
    setLaunching(id);
    try {
      const session = await api.launchGame(id);
      setStatus((current) => current ? { ...current, session } : current);
      onNotice("已打开 MCDK 启动器");
    } catch (cause) { onNotice(errorMessage(cause)); }
    finally { pending.current = false; setLaunching(null); await refresh(); }
  };

  const update = async (action: () => Promise<McdkStatus>) => {
    try { accept(await action()); }
    catch (cause) { await refresh(); setError(errorMessage(cause)); }
  };
  return { status, launching, error, launch, update, refresh };
}

export type McdkController = ReturnType<typeof useMcdk>;

export function LaunchGameButton({ componentId, name, mcdk }: { componentId: string; name: string; mcdk: McdkController }) {
  const waiting = mcdk.launching === componentId;
  const running = mcdk.status?.session?.component_id === componentId;
  const title = running ? "MCDK 会话运行中" : mcdk.status?.session ? "请先退出正在运行的 MCDK 会话" : "启动游戏";
  return <button className={running ? "launch-game active" : "launch-game"} title={title}
    aria-label={"启动游戏 " + name} aria-busy={waiting}
    disabled={!desktop || !mcdk.status?.available || !!mcdk.launching || !!mcdk.status?.session}
    onClick={() => void mcdk.launch(componentId)}>
    {waiting ? <LoaderCircle size={16} className="mcdk-spin" /> : <Play size={16} />}
  </button>;
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
    {status?.session && <p className="mcdk-session">运行中 · v{status.session.version} · PID {status.session.pid}</p>}
    {(mcdk.error || status?.error) && <p className="mcdk-error" role="alert">{mcdk.error || status?.error}</p>}
    <div className="mcdk-actions">
      <label className="mcdk-toggle"><input type="checkbox" role="switch" aria-label="MCDK 自动更新" checked={status?.auto_update ?? true} disabled={!status || saving} onChange={(event) => void toggle(event.target.checked)} /><span>自动更新</span></label>
      <div><button className="button secondary" disabled={!desktop || working} onClick={() => void run(api.checkMcdkUpdate)}><RefreshCw size={15} className={status?.phase === "checking" ? "mcdk-spin" : undefined} />检查更新</button>
        {status?.latest_version && <button className="button secondary" disabled={working} onClick={() => void run(api.installMcdkUpdate)}><Download size={15} />立即更新</button>}</div>
    </div>
  </div>;
}
