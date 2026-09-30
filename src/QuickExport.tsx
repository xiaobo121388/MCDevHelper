import { open } from "@tauri-apps/plugin-dialog";
import { Archive, LoaderCircle, FolderOpen } from "lucide-react";
import { useRef, useState } from "react";
import { api, desktop, errorMessage } from "./api";
import { notifyWorkspace } from "./windows";
import type { AppSettings, ComponentSummary, OperationResult, QuickExportPhase, QuickExportSettings } from "./types";

export const DEFAULT_QUICK_EXPORT: QuickExportSettings = {
  destination: null, regenerate_uuids: true, bump_version: true,
  version_part: "patch", content_mode: "clean", conflict_policy: "rename",
};
const phaseText: Record<QuickExportPhase, string> = { preparing: "准备导出", uuid: "刷新 UUID", version: "提升版本", exporting: "打包 ZIP" };

export function useQuickExport(onDone: (operation: OperationResult, message: string) => void, onSettings: (settings: AppSettings) => void, onNotice: (message: string) => void) {
  const [busyId, setBusyId] = useState<string | null>(null);
  const [phase, setPhase] = useState<QuickExportPhase>("preparing");
  const pending = useRef(false);
  const run = async (component: ComponentSummary) => {
    if (pending.current || !desktop) return;
    pending.current = true; setBusyId(component.id); setPhase("preparing");
    try {
      const saved = await api.settings();
      let destination = saved.quick_export?.destination;
      if (!destination) {
        const chosen = await open({ title: "选择一键导出目录", directory: true, multiple: false });
        if (typeof chosen !== "string") return;
        destination = chosen;
        onSettings(await api.setQuickExportDestination(destination));
      }
      const operation = await api.quickExport(component.id, destination, setPhase);
      const message = `已导出到 ${operation.actual_path}${operation.warnings.length ? "；" + operation.warnings.join("；") : ""}`;
      onDone(operation, message);
      try { await notifyWorkspace(undefined, operation, false); }
      catch (cause) { onNotice(message + "；窗口同步失败：" + errorMessage(cause)); }
    } catch (error) { onNotice(errorMessage(error)); }
    finally { pending.current = false; setBusyId(null); }
  };
  return { run, busyId, phase, status: phaseText[phase] };
}

export type QuickExportController = ReturnType<typeof useQuickExport>;

export function QuickExportButton({ component, settings, controller, running }: { component: ComponentSummary; settings: AppSettings; controller: QuickExportController; running: boolean }) {
  const options = settings.quick_export ?? DEFAULT_QUICK_EXPORT;
  const busy = controller.busyId === component.id;
  const blocked = running && (options.regenerate_uuids || options.bump_version);
  const title = busy ? phaseText[controller.phase] : blocked ? "游戏会话运行中，无法修改包配置" : "一键导出";
  return <button className={busy ? "quick-export-button active" : "quick-export-button"} title={title} aria-label={`一键导出 ${component.name}`} aria-busy={busy}
    disabled={!desktop || !!controller.busyId || blocked} onClick={() => void controller.run(component)}>
    {busy ? <LoaderCircle size={16} className="spin" /> : <Archive size={16} />}
  </button>;
}

export function QuickExportOptions({ value, onChange, disabled }: { value: QuickExportSettings; onChange: (value: QuickExportSettings) => void; disabled: boolean }) {
  const [error, setError] = useState("");
  const update = (changes: Partial<QuickExportSettings>) => onChange({ ...value, ...changes });
  const choose = async () => {
    try {
      setError("");
      const path = await open({ title: "选择一键导出目录", directory: true, multiple: false });
      if (typeof path === "string") update({ destination: path });
    } catch (cause) { setError(errorMessage(cause)); }
  };
  return <fieldset disabled={disabled} className="quick-export-settings">
    <div className="section-heading"><h3>一键导出</h3><Archive size={19} /></div>
    <label className="field"><span>导出目录</span><div className="path-row"><input value={value.destination ?? ""} placeholder="未设置" onChange={(event) => update({ destination: event.target.value || null })} /><button type="button" title="选择导出目录" aria-label="选择导出目录" onClick={() => void choose()}><FolderOpen size={16} /></button></div></label>
    <label className="quick-export-toggle"><input type="checkbox" checked={value.regenerate_uuids} onChange={(event) => update({ regenerate_uuids: event.target.checked })} /><span>刷新 Manifest UUID</span></label>
    <div className="quick-export-version"><label className="quick-export-toggle"><input type="checkbox" checked={value.bump_version} onChange={(event) => update({ bump_version: event.target.checked })} /><span>提升包版本</span></label><select aria-label="一键导出版本提升方式" disabled={!value.bump_version} value={value.version_part} onChange={(event) => update({ version_part: event.target.value as QuickExportSettings["version_part"] })}><option value="patch">Patch</option><option value="minor">Minor</option><option value="major">Major</option></select></div>
    <label className="field"><span>ZIP 内容</span><select value={value.content_mode} onChange={(event) => update({ content_mode: event.target.value as QuickExportSettings["content_mode"] })}><option value="clean">游戏 ZIP</option><option value="full">完整 ZIP</option></select></label>
    <label className="field"><span>同名文件</span><select value={value.conflict_policy} onChange={(event) => update({ conflict_policy: event.target.value as QuickExportSettings["conflict_policy"] })}><option value="rename">追加序号</option><option value="overwrite">覆盖原文件</option><option value="error">停止并提示</option></select></label>
    {error && <p className="form-error" role="alert">{error}</p>}
  </fieldset>;
}
