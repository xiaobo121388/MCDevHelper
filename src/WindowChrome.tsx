import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, PanelsTopLeft, Square, X } from "lucide-react";
import { useEffect, useState } from "react";

export function WindowChrome({ title = "MCDevHelper", status, busy = false, onError }: {
  title?: string; status?: string; busy?: boolean; onError?: (message: string) => void;
}) {
  const native = isTauri();
  const [maximized, setMaximized] = useState(false);
  const [focused, setFocused] = useState(true);
  useEffect(() => {
    if (!native) return;
    const current = getCurrentWindow();
    let active = true;
    const report = (error: unknown) => { if (active) onError?.(String(error)); };
    const update = () => { void current.isMaximized().then((value) => { if (active) setMaximized(value); }).catch(report); };
    update();
    const resize = current.onResized(update);
    const focus = current.onFocusChanged(({ payload }) => { if (active) setFocused(payload); });
    return () => { active = false; void resize.then((stop) => stop()).catch(report); void focus.then((stop) => stop()).catch(report); };
  }, [native, onError]);
  const action = (operation: "minimize" | "toggleMaximize" | "close") => {
    if (!native) return;
    void getCurrentWindow()[operation]().catch((error) => onError?.(String(error)));
  };
  return <header className={`window-chrome${focused ? "" : " unfocused"}`}>
    <div className="window-drag-area" data-tauri-drag-region>
      <span className="window-brand" aria-hidden="true"><PanelsTopLeft size={15} /></span>
      <span className="window-title">{title}</span>
      {status && <span className="window-status" role="status"><i className={busy ? "busy" : ""} />{status}</span>}
    </div>
    <div className="window-controls">
      <button aria-label="最小化" title="最小化" disabled={!native} onClick={() => action("minimize")}><Minus size={16} /></button>
      <button aria-label={maximized ? "还原窗口" : "最大化"} title={maximized ? "还原窗口" : "最大化"} disabled={!native} onClick={() => action("toggleMaximize")}>{maximized ? <PanelsTopLeft size={13} /> : <Square size={13} />}</button>
      <button className="window-close" aria-label="关闭窗口" title="关闭窗口" disabled={!native || busy} onClick={() => action("close")}><X size={17} /></button>
    </div>
  </header>;
}
