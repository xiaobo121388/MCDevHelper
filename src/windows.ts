import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import type { AppSettings, ComponentSummary, OperationResult, UpdateCheckResult } from "./types";

export type StartupDialog =
  | { kind: "updated"; currentVersion: string; notes: string[] }
  | { kind: "available"; update: UpdateCheckResult };
export type DialogRequest =
  | { kind: "settings" | "create" | "import" | "warnings" }
  | { kind: "component"; componentId: string; initialComponent?: ComponentSummary; initialSettings?: AppSettings }
  | { kind: "startup"; dialog: StartupDialog }
  | { kind: "confirm"; message: string; token: string; owner: string };

declare global {
  interface Window { __MCDH_DIALOG__?: DialogRequest }
}

export const nativeWindows = isTauri();
export const dialogRequest = window.__MCDH_DIALOG__;
export const WORKSPACE_CHANGED = "mcdh:workspace-changed";
export const SETTINGS_CHANGED = "mcdh:settings-changed";
export type WorkspaceChange = { message?: string; operation?: OperationResult; refreshAfter?: boolean };

export async function notifyWorkspace(message?: string, operation?: OperationResult, refreshAfter = true) {
  if (nativeWindows) await emit(WORKSPACE_CHANGED, { message, operation, refreshAfter });
}

export async function notifySettings() {
  if (nativeWindows) await emit(SETTINGS_CHANGED);
}

const opening = new Map<string, Promise<Window>>();

export function openDialogWindow(request: DialogRequest): Promise<Window> {
  const key = JSON.stringify(request);
  const pending = opening.get(key);
  if (pending) return pending;
  const created = (async () => {
    const label = await invoke<string>("open_dialog_window", { request });
    const child = await Window.getByLabel(label);
    if (!child) throw new Error("窗口已关闭，请重试。");
    return child;
  })().finally(() => opening.delete(key));
  opening.set(key, created);
  return created;
}

export async function closeDialogWindow() {
  if (nativeWindows) await getCurrentWindow().close();
}

export async function confirmAction(message: string): Promise<boolean> {
  if (!nativeWindows) return window.confirm(message);
  const token = crypto.randomUUID();
  let resolve!: (value: boolean) => void;
  const answer = new Promise<boolean>((done) => { resolve = done; });
  const stop = await listen<boolean>(`mcdh:confirm-${token}`, (event) => resolve(event.payload));
  let stopClosed: (() => void) | undefined;
  try {
    const child = await openDialogWindow({ kind: "confirm", message, token, owner: getCurrentWindow().label });
    stopClosed = await child.once("tauri://destroyed", () => resolve(false));
    return await answer;
  } catch (cause) {
    await notifyWorkspace("无法打开确认窗口：" + String(cause));
    return false;
  } finally {
    stop();
    stopClosed?.();
  }
}
