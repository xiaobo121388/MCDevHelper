import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ settings: vi.fn(), quickExport: vi.fn(), destination: vi.fn(), open: vi.fn(), notify: vi.fn(), done: vi.fn(), notice: vi.fn(), profiles: vi.fn(), quickCustom: vi.fn(), task: vi.fn(), cancel: vi.fn(), resolve: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("./api", () => ({ desktop: true, errorMessage: (value: unknown) => String(value), api: { settings: mocks.settings, quickExport: mocks.quickExport, setQuickExportDestination: mocks.destination, customExportProfiles: mocks.profiles, quickCustomExport: mocks.quickCustom, customExportTask: mocks.task, cancelCustomExport: mocks.cancel, resolveCustomExportConflict: mocks.resolve } }));
vi.mock("./windows", () => ({ notifyWorkspace: mocks.notify }));

import { DEFAULT_QUICK_EXPORT, QuickExportButton, QuickExportOptions, useQuickExport } from "./QuickExport";
import { CustomExportTaskPanel } from "./CustomExport";
import type { AppSettings, ComponentSummary, CustomExportProfile, CustomExportTask, QuickExportSettings } from "./types";

const component: ComponentSummary = { id: "one", name: "Test", kind: "addon", path: "D:/Project", origin: { kind: "library", source_id: "lib" }, manifests: [], tags: [], favorite: false, size_bytes: 1 };
const settings: AppSettings = { developer_nickname: "MCDH", developer_account: "local", developer_user_id: "0", theme: "dark", color_preset: "fluent", quick_export: DEFAULT_QUICK_EXPORT };
const operation = { actual_path: "D:/Exports/Test.zip", component: { ...component, version: [1, 0, 1] }, modified_files: [], warnings: [] };
const profile: CustomExportProfile = { id: "custom", name: "自定义发布", enabled: true, executable: "D:/pack.exe", arguments: [], input_mode: "snapshot", working_directory: null, component_kinds: ["addon"], timeout_seconds: 1800, log_encoding: "utf8", allow_mcp: false };
const task: CustomExportTask = { id: "task", component_id: "one", profile_id: "custom", profile_name: "自定义发布", destination: "D:/Exports", status: "running", cancel_requested: false, conflict_path: null, result: null, error: null, logs: [], next_cursor: 0, logs_truncated: false };

function Workspace({ running = false, options = DEFAULT_QUICK_EXPORT }: { running?: boolean; options?: QuickExportSettings }) {
  const [value, setValue] = useState({ ...settings, quick_export: options });
  const controller = useQuickExport(mocks.done, setValue, mocks.notice);
  return <><QuickExportButton component={component} settings={value} controller={controller} running={running} /><QuickExportButton component={{ ...component, id: "two", name: "Other" }} settings={value} controller={controller} running={false} /><CustomExportTaskPanel controller={controller} /></>;
}

describe("one-click export", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.settings.mockResolvedValue(settings);
    mocks.destination.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Exports" } });
    mocks.quickExport.mockResolvedValue(operation);
    mocks.notify.mockResolvedValue(undefined);
    mocks.open.mockResolvedValue("D:/Exports");
    mocks.profiles.mockResolvedValue([]);
    mocks.quickCustom.mockResolvedValue(task);
    mocks.task.mockResolvedValue(task);
  });
  afterEach(cleanup);

  it("chooses and remembers a missing directory before exporting", async () => {
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    await waitFor(() => expect(mocks.done).toHaveBeenCalledOnce());
    expect(mocks.open).toHaveBeenCalledWith({ title: "选择一键导出目录", directory: true, multiple: false });
    expect(mocks.destination).toHaveBeenCalledWith("D:/Exports");
    expect(mocks.destination.mock.invocationCallOrder[0]).toBeLessThan(mocks.quickExport.mock.invocationCallOrder[0]);
    expect(mocks.quickExport).toHaveBeenCalledWith("one", "D:/Exports", expect.any(Function));
    expect(mocks.done).toHaveBeenCalledWith(operation, "已导出到 D:/Exports/Test.zip");
    expect(mocks.notify).toHaveBeenCalledWith(undefined, operation, false);
    await waitFor(() => expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeEnabled());
  });

  it("uses current saved settings and does not ask again when a path exists", async () => {
    mocks.settings.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Saved" } });
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    await waitFor(() => expect(mocks.quickExport).toHaveBeenCalledWith("one", "D:/Saved", expect.any(Function)));
    expect(mocks.open).not.toHaveBeenCalled();
    expect(mocks.destination).not.toHaveBeenCalled();
  });

  it("cancels directory selection without modifying the project", async () => {
    mocks.open.mockResolvedValue(null);
    render(<Workspace />);
    const button = screen.getByRole("button", { name: "一键导出 Test" });
    fireEvent.click(button);
    await waitFor(() => expect(mocks.open).toHaveBeenCalledOnce());
    await waitFor(() => expect(button).toBeEnabled());
    expect(mocks.destination).not.toHaveBeenCalled();
    expect(mocks.quickExport).not.toHaveBeenCalled();
    expect(mocks.done).not.toHaveBeenCalled();
  });

  it("blocks repeated and parallel clicks and reports the current phase", async () => {
    let complete!: (value: unknown) => void;
    mocks.quickExport.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    render(<Workspace />);
    const button = screen.getByRole("button", { name: "一键导出 Test" });
    fireEvent.click(button); fireEvent.click(button);
    await waitFor(() => expect(mocks.quickExport).toHaveBeenCalledOnce());
    expect(screen.getByRole("button", { name: "一键导出 Other" })).toBeDisabled();
    act(() => mocks.quickExport.mock.calls[0][2]("version"));
    expect(button).toHaveAttribute("title", "提升版本");
    expect(button).toHaveAttribute("aria-busy", "true");
    await act(async () => complete(operation));
    expect(button).toBeEnabled();
  });

  it("reports failures and releases the button for a new attempt", async () => {
    mocks.quickExport.mockRejectedValueOnce(new Error("write denied"));
    render(<Workspace />);
    const button = screen.getByRole("button", { name: "一键导出 Test" });
    fireEvent.click(button);
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith("Error: write denied"));
    expect(button).toBeEnabled();
    expect(mocks.done).not.toHaveBeenCalled();
    fireEvent.click(button);
    await waitFor(() => expect(mocks.done).toHaveBeenCalledOnce());
  });

  it("does not export when saving the chosen destination fails", async () => {
    mocks.destination.mockRejectedValue(new Error("invalid directory"));
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    await waitFor(() => expect(mocks.notice).toHaveBeenCalled());
    expect(mocks.quickExport).not.toHaveBeenCalled();
  });

  it("blocks manifest changes during a game but permits a pure ZIP export", async () => {
    const view = render(<Workspace running />);
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeDisabled();
    view.rerender(<Workspace running options={{ ...DEFAULT_QUICK_EXPORT, bump_version: false, regenerate_uuids: false }} key="pure" />);
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeEnabled();
  });

  it("keeps a successful export visible even when window synchronization fails", async () => {
    mocks.notify.mockRejectedValue(new Error("window closed"));
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith(expect.stringContaining("已导出到 D:/Exports/Test.zip；窗口同步失败")));
    expect(mocks.done).toHaveBeenCalledOnce();
  });

  it("edits all export options and keeps the version selector tied to its checkbox", async () => {
    function Options() {
      const [value, setValue] = useState(DEFAULT_QUICK_EXPORT);
      return <QuickExportOptions value={value} disabled={false} onChange={(next) => { mocks.done(next); setValue(next); }} />;
    }
    render(<Options />);
    expect(screen.getByRole("checkbox", { name: "刷新 Manifest UUID" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "提升包版本" })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "选择导出目录" }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "导出目录" })).toHaveValue("D:/Exports"));
    fireEvent.click(screen.getByRole("checkbox", { name: "刷新 Manifest UUID" }));
    fireEvent.change(screen.getByRole("combobox", { name: "一键导出版本提升方式" }), { target: { value: "minor" } });
    fireEvent.change(screen.getByRole("combobox", { name: "ZIP 内容" }), { target: { value: "full" } });
    fireEvent.change(screen.getByRole("combobox", { name: "同名文件" }), { target: { value: "overwrite" } });
    expect(mocks.done).toHaveBeenLastCalledWith({ destination: "D:/Exports", custom_profile_id: null, regenerate_uuids: false, bump_version: true, version_part: "minor", content_mode: "full", conflict_policy: "overwrite" });
    fireEvent.click(screen.getByRole("checkbox", { name: "提升包版本" }));
    expect(screen.getByRole("combobox", { name: "一键导出版本提升方式" })).toBeDisabled();
  });

  it("uses the saved custom exporter and incrementally reports its complete result", async () => {
    mocks.settings.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Exports", custom_profile_id: "custom" } });
    mocks.task.mockResolvedValue({ ...task, status: "succeeded", result: operation, logs: [{ sequence: 1, source: "stdout", text: "custom packing complete" }], next_cursor: 1 });
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    await waitFor(() => expect(mocks.done).toHaveBeenCalledOnce());
    expect(mocks.quickCustom).toHaveBeenCalledWith("one", "D:/Exports");
    expect(mocks.quickExport).not.toHaveBeenCalled();
    expect(mocks.done).toHaveBeenCalledWith(operation, "已导出到 D:/Exports/Test.zip");
    expect(mocks.notify).toHaveBeenCalledWith(undefined, operation, false);
    expect(screen.getByText("custom packing complete")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "关闭导出任务" }));
    expect(screen.queryByRole("region", { name: "自定义导出任务" })).not.toBeInTheDocument();
  });

  it("keeps custom tasks busy and allows cancellation with logs preserved", async () => {
    mocks.settings.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Exports", custom_profile_id: "custom" } });
    mocks.task.mockResolvedValue({ ...task, logs: [{ sequence: 1, source: "stdout", text: "still packing" }], next_cursor: 1 });
    mocks.cancel.mockResolvedValue({ ...task, cancel_requested: true });
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    expect(await screen.findByText("still packing")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "一键导出 Other" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toHaveAttribute("title", "正在执行");
    mocks.task.mockResolvedValue({ ...task, status: "cancelled", next_cursor: 1, error: { code: "cancelled", message: "自定义导出已取消", exit_code: null } });
    fireEvent.click(screen.getByRole("button", { name: "取消任务" }));
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith("自定义导出已取消"));
    expect(mocks.cancel).toHaveBeenCalledWith("task");
    expect(screen.getByText("still packing")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeEnabled();
    expect(mocks.done).not.toHaveBeenCalled();
  });

  it("lets polling reconnect without starting another export", async () => {
    mocks.settings.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Exports", custom_profile_id: "custom" } });
    mocks.task.mockRejectedValueOnce(new Error("connection lost"));
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    const retry = await screen.findByRole("button", { name: "重新连接" });
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeDisabled();
    mocks.task.mockResolvedValue({ ...task, status: "succeeded", result: operation });
    fireEvent.click(retry);
    await waitFor(() => expect(mocks.done).toHaveBeenCalledOnce());
    expect(mocks.quickCustom).toHaveBeenCalledOnce();
    expect(screen.queryByText("Error: connection lost")).not.toBeInTheDocument();
  });

  it("reports missing custom profiles without falling back to ZIP", async () => {
    mocks.settings.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Exports", custom_profile_id: "gone" } });
    mocks.quickCustom.mockRejectedValue(new Error("profile not found"));
    render(<Workspace />);
    fireEvent.click(screen.getByRole("button", { name: "一键导出 Test" }));
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith("Error: profile not found"));
    expect(mocks.quickExport).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "一键导出 Test" })).toBeEnabled();
  });

  it("reports failed tasks and permits retrying after rollback completes", async () => {
    mocks.settings.mockResolvedValue({ ...settings, quick_export: { ...DEFAULT_QUICK_EXPORT, destination: "D:/Exports", custom_profile_id: "custom" } });
    mocks.task.mockResolvedValue({ ...task, status: "failed", error: { code: "execution_failed", message: "打包失败", exit_code: 7 } });
    render(<Workspace />);
    const button = screen.getByRole("button", { name: "一键导出 Test" });
    fireEvent.click(button);
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith("打包失败"));
    expect(button).toBeEnabled();
    expect(screen.getByRole("alert")).toHaveTextContent("退出码 7");
    mocks.quickCustom.mockResolvedValue({ ...task, id: "retry" });
    mocks.task.mockResolvedValue({ ...task, id: "retry", status: "succeeded", result: operation });
    fireEvent.click(button);
    await waitFor(() => expect(mocks.done).toHaveBeenCalledOnce());
    expect(mocks.quickCustom).toHaveBeenCalledTimes(2);
  });

  it("selects enabled custom profiles and retains ZIP options when switching back", async () => {
    mocks.profiles.mockResolvedValue([profile, { ...profile, id: "disabled", name: "禁用方案", enabled: false }]);
    function Options() {
      const [value, setValue] = useState({ ...DEFAULT_QUICK_EXPORT, content_mode: "full" as const });
      return <QuickExportOptions value={value} disabled={false} onChange={(next) => { mocks.done(next); setValue(next as typeof value); }} />;
    }
    render(<Options />);
    await screen.findByRole("option", { name: "自定义发布" });
    expect(screen.queryByRole("option", { name: "禁用方案" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "导出方式" }), { target: { value: "custom" } });
    expect(mocks.done).toHaveBeenLastCalledWith({ ...DEFAULT_QUICK_EXPORT, content_mode: "full", custom_profile_id: "custom" });
    expect(screen.queryByRole("combobox", { name: "ZIP 内容" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "导出方式" }), { target: { value: "" } });
    expect(screen.getByRole("combobox", { name: "ZIP 内容" })).toHaveValue("full");
  });

  it("keeps unavailable selected profiles explicit rather than silently resetting", async () => {
    mocks.profiles.mockResolvedValue([{ ...profile, enabled: false }]);
    const view = render(<QuickExportOptions value={{ ...DEFAULT_QUICK_EXPORT, custom_profile_id: "custom" }} disabled={false} onChange={mocks.done} />);
    expect(await screen.findByRole("option", { name: "自定义发布（已停用）" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "导出方式" })).toHaveValue("custom");
    view.rerender(<QuickExportOptions value={{ ...DEFAULT_QUICK_EXPORT, custom_profile_id: "removed" }} disabled={false} onChange={mocks.done} />);
    expect(screen.getByRole("option", { name: "自定义方案（不可用）" })).toBeDisabled();
    expect(mocks.done).not.toHaveBeenCalled();
  });
});
