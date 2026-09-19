import { Download, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, desktop, errorMessage } from "./api";
import type { AppUpdateProgress } from "./types";

export function useAppUpdate() {
  const [progress, setProgress] = useState<AppUpdateProgress | null>(null);
  const [error, setError] = useState("");
  const running = useRef(false);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void api.appUpdateError().then((message) => {
      if (active && message && !running.current) setError(message);
    }).catch(() => undefined);
    return () => { active = false; };
  }, []);
  const install = async (version: string) => {
    if (!desktop || running.current) return;
    running.current = true;
    setError("");
    setProgress({ phase: "checking", downloaded_bytes: 0, total_bytes: 0 });
    try {
      await api.installAppUpdate(version, setProgress);
      setProgress((value) => ({ ...value!, phase: "installing" }));
    } catch (failure) {
      setError(errorMessage(failure));
      setProgress(null);
      running.current = false;
    }
  };
  const clearError = () => {
    setError("");
    if (desktop) void api.appUpdateError(true).catch(() => undefined);
  };
  return { progress, error, install, clearError };
}

export type AppUpdater = ReturnType<typeof useAppUpdate>;

export function UpdateButton({ updater, version }: { updater: AppUpdater; version?: string }) {
  return <button className="button primary" disabled={!desktop || !version || !!updater.progress}
    onClick={() => version && void updater.install(version)}><Download size={16} />立即更新</button>;
}

export function UpdateProgress({ progress }: { progress: AppUpdateProgress }) {
  const labels = { checking: "正在确认更新", downloading: "正在下载更新", verifying: "正在校验更新", installing: "正在安装，即将自动重启" };
  const percent = progress.total_bytes ? Math.min(100, Math.floor(progress.downloaded_bytes / progress.total_bytes * 100)) : 0;
  return <div className="modal-backdrop app-update-overlay" role="presentation">
    <section className="modal" role="dialog" aria-modal="true" aria-labelledby="app-update-title">
      <header><h2 id="app-update-title">{labels[progress.phase]}</h2><RefreshCw className="spin" size={19} /></header>
      <div className="modal-content app-update-progress" role="status">
        <progress aria-label="更新下载进度" max={100} value={progress.phase === "checking" ? undefined : percent} />
        <p>{progress.phase === "downloading" ? percent + "%" : labels[progress.phase]}</p>
        <p>下载校验完成后将自动安装并重启，无需操作安装向导。请勿关闭应用。</p>
      </div>
    </section>
  </div>;
}
