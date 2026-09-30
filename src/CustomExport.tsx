import { useCallback, useEffect, useRef, useState } from "react";
import { Archive, RefreshCw, Square, Terminal, X } from "lucide-react";
import { api, errorMessage } from "./api";
import type { ComponentSummary, CustomExportProfile, CustomExportStatus, CustomExportTask, ExportLog, OperationResult } from "./types";

export const exportTerminal = (status: CustomExportStatus) => ["succeeded", "failed", "cancelled"].includes(status);
const statusText: Record<CustomExportStatus, string> = {
  preparing: "准备输入", running: "正在执行", validating: "检查产物", awaiting_conflict: "等待处理重名",
  publishing: "保存产物", succeeded: "导出成功", failed: "导出失败", cancelled: "已取消",
};
export const exportTaskStatus = (task: CustomExportTask) => task.cancel_requested && !exportTerminal(task.status) ? "正在取消…" : statusText[task.status];

export function useExportTask(onDone: (operation: OperationResult, destination: string) => void, onError: (message: string) => void, onSettled?: (task: CustomExportTask | null) => void) {
  const [task, setTask] = useState<CustomExportTask | null>(null);
  const [logs, setLogs] = useState<ExportLog[]>([]);
  const [truncated, setTruncated] = useState(false);
  const [pending, setPending] = useState(false);
  const [pollError, setPollError] = useState("");
  const [retry, setRetry] = useState(0);
  const cursor = useRef(0);
  const logBuffer = useRef<ExportLog[]>([]);
  const completed = useRef(new Set<string>());
  const starting = useRef(false);
  const alive = useRef(true);
  const currentTask = useRef(task);
  currentTask.current = task;
  const callbacks = useRef({ onDone, onError, onSettled });
  callbacks.current = { onDone, onError, onSettled };
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const attach = useCallback((next: CustomExportTask, restored = false) => {
    // Restored terminal tasks are history, not new completion events.
    if (restored && exportTerminal(next.status)) completed.current.add(next.id);
    cursor.current = 0; logBuffer.current = []; currentTask.current = next;
    setLogs([]); setTruncated(false); setPollError(""); setTask(next);
  }, []);

  useEffect(() => {
    if (!task?.id) return;
    const id = task.id;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = await api.customExportTask(id, cursor.current);
        if (!active) return;
        setPollError(""); setTask(next); cursor.current = next.next_cursor;
        const combined = [...logBuffer.current, ...next.logs];
        let size = combined.reduce((sum, log) => sum + new TextEncoder().encode(log.text).length, 0);
        let start = 0;
        while (size > 1024 * 1024 && start < combined.length) { size -= new TextEncoder().encode(combined[start++].text).length; }
        logBuffer.current = combined.slice(start);
        setLogs(logBuffer.current);
        if (next.logs_truncated || start > 0) setTruncated(true);
        if (exportTerminal(next.status)) {
          if (!completed.current.has(id)) {
            completed.current.add(id);
            if (next.status === "succeeded" && next.result) callbacks.current.onDone(next.result, next.destination);
            callbacks.current.onSettled?.(next);
          }
        } else { timer = setTimeout(() => void poll(), 500); }
      } catch (error) {
        if (active) {
          if (error && typeof error === "object" && "code" in error && error.code === "task_not_found") {
            setTask(null); callbacks.current.onError(errorMessage(error));
            callbacks.current.onSettled?.(null);
          } else setPollError(errorMessage(error));
        }
      }
    };
    void poll();
    return () => { active = false; clearTimeout(timer); };
  }, [task?.id, retry]);

  const start = async (operation: () => Promise<CustomExportTask>) => {
    if (starting.current || currentTask.current && !exportTerminal(currentTask.current.status)) return false;
    starting.current = true; setPending(true);
    try {
      const next = await operation();
      if (alive.current) attach(next);
      return true;
    } catch (error) { callbacks.current.onError(errorMessage(error)); return false; }
    finally { starting.current = false; if (alive.current) setPending(false); }
  };
  const action = async (operation: () => Promise<CustomExportTask>) => {
    setPending(true);
    try { const next = await operation(); if (alive.current) { setTask(next); setRetry((value) => value + 1); } }
    catch (error) { callbacks.current.onError(errorMessage(error)); }
    finally { if (alive.current) setPending(false); }
  };
  return {
    task, logs, truncated, pending, pollError, start, attach,
    busy: pending || !!task && !exportTerminal(task.status),
    retry: () => setRetry((value) => value + 1),
    cancel: () => task && void action(() => api.cancelCustomExport(task.id)),
    resolve: (policy: "rename" | "overwrite") => task && void action(() => api.resolveCustomExportConflict(task.id, policy)),
    dismiss: () => {
      if (currentTask.current && !exportTerminal(currentTask.current.status)) return;
      currentTask.current = null; cursor.current = 0; logBuffer.current = [];
      setTask(null); setLogs([]); setTruncated(false); setPollError("");
    },
  };
}

export function useCustomExport(component: ComponentSummary, destination: string, onDone: (operation: OperationResult, destination: string) => void, onError: (message: string) => void) {
  const [profiles, setProfiles] = useState<CustomExportProfile[]>([]);
  const controller = useExportTask(onDone, onError);
  const { attach } = controller;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  useEffect(() => {
    let active = true;
    Promise.all([api.customExportProfiles(), api.customExportTasks()]).then(([nextProfiles, tasks]) => {
      if (!active) return;
      setProfiles(nextProfiles.filter((profile) => profile.enabled && profile.component_kinds.includes(component.kind)));
      const matching = tasks.filter((item) => item.component_id === component.id);
      const existing = matching.find((item) => !exportTerminal(item.status)) ?? matching.at(-1);
      if (existing) attach(existing, true);
    }).catch((error) => { if (active) onErrorRef.current(errorMessage(error)); });
    return () => { active = false; };
  }, [component.id, component.kind, attach]);
  const start = async (profile: CustomExportProfile) => {
    if (!destination) { onErrorRef.current("请先选择导出目录。"); return; }
    await controller.start(() => api.startCustomExport({ component_id: component.id, profile_id: profile.id, destination, conflict_policy: "error" }));
  };
  return { ...controller, profiles, start };
}

export type CustomExportController = ReturnType<typeof useCustomExport>;
export type ExportTaskController = Pick<ReturnType<typeof useExportTask>, "task" | "logs" | "truncated" | "pending" | "pollError" | "cancel" | "resolve" | "retry" | "dismiss">;

export function CustomExportButtons({ controller, disabled, gameRunning }: { controller: CustomExportController; disabled: boolean; gameRunning: boolean }) {
  return <>{controller.profiles.map((profile) => <button key={profile.id} className="button secondary custom-export-button" disabled={disabled || controller.busy || gameRunning && profile.input_mode === "source"} onClick={() => void controller.start(profile)} title={profile.name}><Archive size={16} />{profile.name}</button>)}</>;
}

export function CustomExportTaskPanel({ controller }: { controller: ExportTaskController }) {
  const { task, logs, truncated, pending, pollError } = controller;
  const logPane = useRef<HTMLPreElement>(null);
  const followLogs = useRef(true);
  useEffect(() => { followLogs.current = true; }, [task?.id]);
  useEffect(() => {
    if (followLogs.current && logPane.current) logPane.current.scrollTop = logPane.current.scrollHeight;
  }, [logs]);
  if (!task) return null;
  return <section className="export-task" aria-label="自定义导出任务">
    <div className="export-task-heading"><strong><Terminal size={16} />{task.profile_name}</strong><span role="status">{exportTaskStatus(task)}</span>
      {!exportTerminal(task.status) && <button className="button secondary" disabled={pending || task.cancel_requested} onClick={controller.cancel}><Square size={14} />取消任务</button>}
      {exportTerminal(task.status) && <button className="icon-button" title="关闭导出任务" aria-label="关闭导出任务" onClick={controller.dismiss}><X size={16} /></button>}
    </div>
    {task.status === "awaiting_conflict" && <div className="export-conflict" role="alert"><div><strong>导出文件已存在</strong><p title={task.conflict_path ?? ""}>{task.conflict_path}</p></div><div className="export-conflict-actions"><button className="button secondary" disabled={pending || task.cancel_requested} onClick={() => controller.resolve("rename")}>添加后缀</button><button className="button danger" disabled={pending || task.cancel_requested} onClick={() => controller.resolve("overwrite")}>覆盖原文件</button></div></div>}
    {task.error && <p className="form-error" role="alert">{task.error.message}{task.error.exit_code != null && '（退出码 ' + task.error.exit_code + '）'}</p>}
    {task.result && <p className="export-result-path" title={task.result.actual_path}>{task.result.actual_path}</p>}
    {truncated && <p className="export-security-note">日志已截断，仅保留最近 1 MiB。</p>}
    <pre ref={logPane} className="export-task-log" aria-label="导出日志" onScroll={(event) => {
      const element = event.currentTarget;
      followLogs.current = element.scrollHeight - element.scrollTop - element.clientHeight < 24;
    }}>{logs.map((log) => <span key={log.sequence} data-source={log.source}>{log.text}</span>)}</pre>
    {pollError && <div role="alert" className="export-poll-error"><span>{pollError}</span><button className="button secondary" onClick={controller.retry}><RefreshCw size={14} />重新连接</button></div>}
  </section>;
}
