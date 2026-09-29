import { useEffect, useRef, useState, type ReactNode } from "react";
import { nativeWindows, openDialogWindow, type DialogRequest } from "./windows";

export function DialogLauncher({ request, onClose, children }: { request: DialogRequest; onClose: () => void; children: ReactNode }) {
  const close = useRef(onClose);
  close.current = onClose;
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const serialized = JSON.stringify(request);
  useEffect(() => {
    if (!nativeWindows) return;
    let active = true;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const child = await openDialogWindow(JSON.parse(serialized) as DialogRequest);
        const unlisten = await child.once("tauri://destroyed", () => { if (active) close.current(); });
        if (active) stop = unlisten;
        else unlisten();
      } catch (cause) { if (active) setError(String(cause)); }
    })();
    return () => { active = false; stop?.(); };
  }, [serialized, retry]);
  if (!nativeWindows) return children;
  if (!error) return null;
  return <div className="toast" role="alert">窗口打开失败：{error}<div className="dialog-actions">
    <button className="button secondary" onClick={() => { setError(""); setRetry((value) => value + 1); }}>重试</button>
    <button className="button secondary" onClick={onClose}>关闭</button>
  </div></div>;
}
