import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  desktop: true,
  refresh: vi.fn(),
  settings: vi.fn(),
  setSettings: vi.fn(),
  delete: vi.fn(),
  export: vi.fn(),
  quickExport: vi.fn(),
  quickCustomExport: vi.fn(),
  setExportSource: vi.fn(),
  exportSource: vi.fn(),
  requestDirectory: vi.fn(),
  setQuickExportDestination: vi.fn(),
  open: vi.fn(),
  customExportProfiles: vi.fn(),
  customExportTasks: vi.fn(),
  startCustomExport: vi.fn(),
  customExportTask: vi.fn(),
  import: vi.fn(),
  create: vi.fn(),
  metadata: vi.fn(),
  regenerateUuids: vi.fn(),
  bumpVersion: vi.fn(),
  openWarningDirectory: vi.fn(),
  removeSource: vi.fn(),
  sources: vi.fn(),
  vscodeStatus: vi.fn(),
  version: vi.fn(),
  checkForUpdates: vi.fn(),
  installAppUpdate: vi.fn(),
  appUpdateError: vi.fn(),
  openUrl: vi.fn(),
  mcdkStatus: vi.fn(),
  onMcdkStatus: vi.fn(),
  launchGame: vi.fn(),
  launchEditor: vi.fn(),
  setMcdeDirectory: vi.fn(),
  setMcdkAutoUpdate: vi.fn(),
  checkMcdkUpdate: vi.fn(),
  installMcdkUpdate: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: mocks.openUrl,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("./windows", async (actual) => ({ ...await actual<typeof import("./windows")>(), requestExportDirectory: mocks.requestDirectory }));

vi.mock("./api", () => ({
  get desktop() { return mocks.desktop; },
  api: {
    refresh: mocks.refresh,
    settings: mocks.settings,
    setSettings: mocks.setSettings,
    delete: mocks.delete,
    export: mocks.export,
    quickExport: mocks.quickExport,
    quickCustomExport: mocks.quickCustomExport,
    setExportSource: mocks.setExportSource,
    exportSource: mocks.exportSource,
    setQuickExportDestination: mocks.setQuickExportDestination,
    customExportProfiles: mocks.customExportProfiles,
    customExportTasks: mocks.customExportTasks,
    startCustomExport: mocks.startCustomExport,
    customExportTask: mocks.customExportTask,
    import: mocks.import,
    create: mocks.create,
    metadata: mocks.metadata,
    regenerateUuids: mocks.regenerateUuids,
    bumpVersion: mocks.bumpVersion,
    openWarningDirectory: mocks.openWarningDirectory,
    removeSource: mocks.removeSource,
    sources: mocks.sources,
    vscodeStatus: mocks.vscodeStatus,
    version: mocks.version,
    checkForUpdates: mocks.checkForUpdates,
    installAppUpdate: mocks.installAppUpdate,
    appUpdateError: mocks.appUpdateError,
    mcdkStatus: mocks.mcdkStatus,
    onMcdkStatus: mocks.onMcdkStatus,
    launchGame: mocks.launchGame,
    launchEditor: mocks.launchEditor,
    setMcdeDirectory: mocks.setMcdeDirectory,
    setMcdkAutoUpdate: mocks.setMcdkAutoUpdate,
    checkMcdkUpdate: mocks.checkMcdkUpdate,
    installMcdkUpdate: mocks.installMcdkUpdate,
  },
  errorMessage: (error: unknown) => String(error),
}));

import { App } from "./App";
import { LaunchGameButton, McdkSettings, useMcdk } from "./Mcdk";
import { releaseNotesFor } from "./releaseNotes";
import type { ComponentSummary, McdkSession, McdkStatus } from "./types";
import type { AppUpdateProgress } from "./types";

const mcdkReady: McdkStatus = {
  available: true, current_version: "1.6.1", auto_update: true, phase: "idle", latest_version: null,
  last_checked_at: null, error: null, downloaded_bytes: 0, download_size: null, session: null, last_exit: null,
  mcde: { available: true, directory: "D:/Bundled/MCDE", configured: false, host_available: true, host_revision: "abc1234", plugin_version: "0.1.0", error: null },
};

const launchComponent: ComponentSummary = {
  id: "game", name: "编辑测试", kind: "addon", path: "D:/Project", origin: { kind: "single" }, manifests: [], tags: [], favorite: false, size_bytes: 1,
};
const editorSession: McdkSession = {
  component_id: "game", component_path: "D:/Project", executable: "D:/Bundled/MCDEHost.exe", version: "1.6.1", pid: 100,
  created_at: "2026-10-06T12:00:00Z", mode: "editor", launch_directory: "D:/Project",
};

describe("component workspace filters", () => {
  afterEach(cleanup);

  beforeEach(() => {
    mocks.desktop = true;
    window.localStorage.clear();
    window.localStorage.setItem("mcdh.last-launched-version", "1.1.0");
    mocks.refresh.mockReset();
    mocks.settings.mockReset().mockResolvedValue({
      developer_nickname: "MCDH",
      developer_account: "mcdh@local.invalid",
      developer_user_id: "0",
      theme: "system",
    });
    mocks.setSettings.mockReset();
    mocks.delete.mockReset().mockResolvedValue({ actual_path: "", modified_files: [], warnings: [] });
    mocks.export.mockReset();
    mocks.quickExport.mockReset();
    mocks.quickCustomExport.mockReset();
    mocks.setExportSource.mockReset().mockResolvedValue("D:/BOMD/src");
    mocks.exportSource.mockReset().mockResolvedValue({ path: "D:/Project", configured: false, valid: true, issue: null });
    mocks.requestDirectory.mockReset().mockResolvedValue("D:/BOMD/src");
    mocks.setQuickExportDestination.mockReset();
    mocks.open.mockReset();
    mocks.customExportProfiles.mockReset().mockResolvedValue([]);
    mocks.customExportTasks.mockReset().mockResolvedValue([]);
    mocks.startCustomExport.mockReset();
    mocks.customExportTask.mockReset();
    mocks.import.mockReset();
    mocks.create.mockReset();
    mocks.metadata.mockReset();
    mocks.regenerateUuids.mockReset();
    mocks.bumpVersion.mockReset();
    mocks.openWarningDirectory.mockReset().mockResolvedValue(undefined);
    mocks.removeSource.mockReset().mockResolvedValue(true);
    mocks.sources.mockReset().mockResolvedValue([]);
    mocks.vscodeStatus.mockReset().mockResolvedValue({ available: false, custom: false });
    mocks.version.mockReset().mockResolvedValue("1.1.0");
    mocks.checkForUpdates.mockReset().mockResolvedValue({
      current_version: "1.1.0",
      latest_version: "v1.1.0",
      release_name: "MCDH 1.1.0",
      update_available: false,
      no_release: false,
    });
    mocks.openUrl.mockReset().mockResolvedValue(undefined);
    mocks.installAppUpdate.mockReset().mockResolvedValue(undefined);
    mocks.appUpdateError.mockReset().mockResolvedValue(null);
    mocks.mcdkStatus.mockReset().mockResolvedValue(mcdkReady);
    mocks.onMcdkStatus.mockReset().mockResolvedValue(() => undefined);
    mocks.launchGame.mockReset();
    mocks.launchEditor.mockReset();
    mocks.setMcdeDirectory.mockReset().mockResolvedValue(mcdkReady);
    mocks.setMcdkAutoUpdate.mockReset().mockResolvedValue({ ...mcdkReady, auto_update: false });
    mocks.checkMcdkUpdate.mockReset().mockResolvedValue(mcdkReady);
    mocks.installMcdkUpdate.mockReset().mockResolvedValue(mcdkReady);
  });

  it("preserves workspace filters when visiting plugin management", async () => {
    mocks.refresh.mockResolvedValue({ components:[launchComponent], sources:[], warnings:[] });
    render(<App />);
    await screen.findByText(launchComponent.name);
    fireEvent.change(screen.getByPlaceholderText("搜索名称或路径"), {target:{value:"编辑"}});
    fireEvent.click(screen.getByRole("button", {name:/^插件/}));
    expect(await screen.findByRole("heading", {name:"插件"})).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", {name:/全部组件/}));
    expect(screen.getByPlaceholderText("搜索名称或路径")).toHaveValue("编辑");
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("places launch before open directory and prevents repeated launch requests", async () => {
    mocks.refresh.mockResolvedValue({ components: [{ id: "game", name: "测试模组", kind: "addon", path: "D:\\作品", origin: { kind: "single" }, manifests: [], tags: [], size_bytes: 1 }], sources: [], warnings: [] });
    let complete!: (value: unknown) => void;
    mocks.launchGame.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    render(<App />);
    const launch = await screen.findByRole("button", { name: "启动游戏 测试模组" });
    await waitFor(() => expect(launch).toBeEnabled());
    expect(launch.previousElementSibling).toBe(screen.getByRole("button", { name: "在 MCDE 中打开 测试模组" }));
    expect(launch.nextElementSibling).toHaveAttribute("title", "打开目录");
    fireEvent.click(launch);
    fireEvent.click(launch);
    expect(mocks.launchGame).toHaveBeenCalledTimes(1);
    expect(mocks.launchGame).toHaveBeenCalledWith("game");
    expect(mocks.launchEditor).not.toHaveBeenCalled();
    expect(launch).toBeDisabled();
    complete({ component_id: "game", pid: 100, version: "1.6.1" });
    expect(await screen.findByText("已打开 MCDK 启动器")).toBeInTheDocument();
  });

  it.each(["editor", "game"] as const)("shares the launch guard when %s is in flight", async (mode) => {
    mocks.refresh.mockResolvedValue({ components: [launchComponent, { ...launchComponent, id: "other", name: "另一个组件" }], sources: [], warnings: [] });
    const session = { ...editorSession, mode };
    const start = mode === "editor" ? mocks.launchEditor : mocks.launchGame;
    let complete!: (value: McdkSession) => void;
    start.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    render(<App />);
    const editor = await screen.findByRole("button", { name: "在 MCDE 中打开 编辑测试" });
    const game = screen.getByRole("button", { name: "启动游戏 编辑测试" });
    await waitFor(() => expect(editor).toBeEnabled());
    expect(editor.nextElementSibling).toBe(game);
    expect(editor).toHaveAttribute("title", "在 MCDE 中打开 编辑测试");
    expect(editor.textContent).toBe("");
    const selected = mode === "editor" ? editor : game;
    fireEvent.click(selected);
    fireEvent.click(selected);
    fireEvent.click(mode === "editor" ? game : editor);
    fireEvent.click(screen.getByRole("button", { name: "在 MCDE 中打开 另一个组件" }));
    fireEvent.click(screen.getByRole("button", { name: "启动游戏 另一个组件" }));
    expect(start).toHaveBeenCalledExactlyOnceWith("game");
    expect(mode === "editor" ? mocks.launchGame : mocks.launchEditor).not.toHaveBeenCalled();
    expect(selected).toHaveAttribute("aria-busy", "true");
    expect(mode === "editor" ? game : editor).toHaveAttribute("aria-busy", "false");
    for (const button of [editor, game, screen.getByRole("button", { name: "在 MCDE 中打开 另一个组件" })]) expect(button).toBeDisabled();
    mocks.mcdkStatus.mockResolvedValue({ ...mcdkReady, session });
    await act(async () => complete(session));
    expect(await screen.findByText(mode === "editor" ? "已启动 MCDE 宿主（游戏就绪状态未确认）" : "已打开 MCDK 启动器")).toBeInTheDocument();
    expect(editor).toBeDisabled();
    expect(game).toBeDisabled();
    expect(mode === "editor" ? editor : game).toHaveClass("active");
    expect(mode === "editor" ? game : editor).not.toHaveClass("active");
  });

  it("reports editor launch errors and allows a retry without launching the game", async () => {
    mocks.refresh.mockResolvedValue({ components: [launchComponent], sources: [], warnings: [] });
    mocks.launchEditor.mockRejectedValueOnce("MCDE 插件启动失败").mockResolvedValue(editorSession);
    render(<App />);
    const editor = await screen.findByRole("button", { name: "在 MCDE 中打开 编辑测试" });
    await waitFor(() => expect(editor).toBeEnabled());
    fireEvent.click(editor);
    expect(await screen.findByText("MCDE 插件启动失败")).toBeInTheDocument();
    await waitFor(() => expect(editor).toBeEnabled());
    mocks.mcdkStatus.mockResolvedValue({ ...mcdkReady, session: editorSession });
    fireEvent.click(editor);
    expect(await screen.findByText("已启动 MCDE 宿主（游戏就绪状态未确认）")).toBeInTheDocument();
    expect(mocks.launchEditor).toHaveBeenCalledTimes(2);
    expect(mocks.launchGame).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "启动游戏 编辑测试" })).toBeDisabled();
  });

  it("can launch the validated editor host independently of the ordinary MCDK binary", async () => {
    const status = { ...mcdkReady, available: false, current_version: null };
    mocks.refresh.mockResolvedValue({ components: [launchComponent], sources: [], warnings: [] });
    mocks.mcdkStatus.mockResolvedValue(status);
    mocks.launchEditor.mockResolvedValue(editorSession);
    render(<App />);
    const editor = await screen.findByRole("button", { name: "在 MCDE 中打开 编辑测试" });
    await waitFor(() => expect(editor).toBeEnabled());
    expect(screen.getByRole("button", { name: "启动游戏 编辑测试" })).toBeDisabled();
    mocks.mcdkStatus.mockResolvedValue({ ...status, session: editorSession });
    fireEvent.click(editor);
    expect(await screen.findByText("已启动 MCDE 宿主（游戏就绪状态未确认）")).toBeInTheDocument();
    expect(mocks.launchEditor).toHaveBeenCalledExactlyOnceWith("game");
    expect(mocks.launchGame).not.toHaveBeenCalled();
  });

  it.each([
    ["missing plugin", { available: false }, "MCDE 插件不可用，请在开发工具中配置插件目录"],
    ["missing host", { available: false, host_available: false, host_revision: null }, "MCDE 宿主不可用"],
    ["invalid plugin", { available: false, error: "MCDE 插件缺少入口文件" }, "MCDE 插件缺少入口文件"],
  ] as const)("disables only the editor launch for a %s and exposes its setup error", async (_reason, mcde, issue) => {
    mocks.refresh.mockResolvedValue({ components: [launchComponent], sources: [], warnings: [] });
    mocks.mcdkStatus.mockResolvedValue({ ...mcdkReady, mcde: { ...mcdkReady.mcde, ...mcde } });
    render(<App />);
    const editor = await screen.findByRole("button", { name: "在 MCDE 中打开 编辑测试" });
    await waitFor(() => expect(editor).toHaveAttribute("title", issue));
    expect(editor).toBeDisabled();
    fireEvent.click(editor);
    expect(mocks.launchEditor).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "启动游戏 编辑测试" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "开发工具" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(issue);
    expect(screen.getByRole("textbox", { name: "MCDE 插件目录" })).toHaveAttribute("aria-invalid", "true");
  });

  it("does not dispatch launches or plugin configuration outside the desktop app", async () => {
    mocks.desktop = false;
    const notice = vi.fn();
    const { result } = renderHook(() => useMcdk(notice));
    const mcdk = { ...result.current, status: mcdkReady };
    render(<><LaunchGameButton componentId="game" name="编辑测试" mcdk={mcdk} /><McdkSettings mcdk={mcdk} /></>);
    const editor = screen.getByRole("button", { name: "在 MCDE 中打开 编辑测试" });
    expect(editor).toBeDisabled();
    expect(editor).toHaveAttribute("title", "请在桌面应用中打开 MCDE");
    expect(screen.getByRole("button", { name: "启动游戏 编辑测试" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "选择 MCDE 插件目录" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复内置 MCDE 插件" })).toBeDisabled();
    await act(async () => { await result.current.launchEditor("game"); await result.current.launch("game"); });
    expect(mocks.launchEditor).not.toHaveBeenCalled();
    expect(mocks.launchGame).not.toHaveBeenCalled();
    expect(mocks.open).not.toHaveBeenCalled();
    expect(mocks.setMcdeDirectory).not.toHaveBeenCalled();
    expect(mocks.mcdkStatus).not.toHaveBeenCalled();
  });

  it.each(["create", "import"] as const)("adds the %s result without refreshing or resetting filters", async (action) => {
    const existing = { id: "existing", name: "旧模组", kind: "addon", path: "D:/Library/Old", origin: { kind: "library" }, tags: ["开发"], manifests: [], favorite: false, size_bytes: 1 };
    const added = { ...existing, id: "added", name: "新模组", path: "D:/Library/New" };
    const sources = [{ id: "lib", path: "D:/Library", kind: "library" }];
    mocks.refresh.mockResolvedValue({ components: [existing], sources, warnings: [] });
    mocks[action].mockResolvedValue({ component: added, actual_path: added.path, modified_files: [], warnings: [] });
    render(<App />);
    await screen.findByRole("heading", { name: "旧模组" });
    fireEvent.change(screen.getByPlaceholderText("搜索名称或路径"), { target: { value: "模组" } });
    fireEvent.click(screen.getByRole("button", { name: action === "create" ? "新建组件" : "导入" }));
    if (action === "create") {
      fireEvent.change(screen.getByRole("textbox", { name: "组件名称" }), { target: { value: "新模组" } });
      fireEvent.click(screen.getByRole("button", { name: "创建组件" }));
    } else {
      fireEvent.change(screen.getByRole("textbox", { name: /^导入来源/ }), { target: { value: "D:/Incoming.zip" } });
      fireEvent.change(screen.getByRole("textbox", { name: /^存放位置/ }), { target: { value: "D:/Library" } });
      fireEvent.click(screen.getByRole("button", { name: "开始导入" }));
    }
    expect(await screen.findByRole("heading", { name: "新模组" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "旧模组" })).toBeInTheDocument();
    expect(screen.getByPlaceholderText("搜索名称或路径")).toHaveValue("模组");
    expect(mocks.refresh).toHaveBeenCalledOnce();
  });

  it("exports from the card footer and updates its version without rescanning", async () => {
    const component = { id: "quick", name: "快捷导出", kind: "addon", path: "D:/Project", origin: { kind: "single" }, version: [1, 0, 0], manifests: [], tags: [], favorite: false, size_bytes: 1 };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.settings.mockResolvedValue({ developer_nickname: "MCDH", developer_account: "local", developer_user_id: "0", theme: "dark", quick_export: { destination: "D:/Exports", regenerate_uuids: true, bump_version: true } });
    let complete!: (value: unknown) => void;
    mocks.quickExport.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    render(<App />);
    const button = await screen.findByRole("button", { name: "一键导出 快捷导出" });
    expect(button.previousElementSibling).toHaveAttribute("title", "用 VS Code 打开");
    fireEvent.click(button);
    await waitFor(() => expect(mocks.quickExport).toHaveBeenCalledOnce());
    expect(screen.getByRole("button", { name: "启动游戏 快捷导出" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "在 MCDE 中打开 快捷导出" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "在 MCDE 中打开 快捷导出" }));
    expect(mocks.launchEditor).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "配置 快捷导出" })).toBeDisabled();
    await act(async () => complete({ actual_path: "D:/Exports/快捷导出.zip", modified_files: [], warnings: [], component: { ...component, version: [1, 0, 1] } }));
    expect(screen.getByText("v1.0.1")).toBeInTheDocument();
    expect(mocks.refresh).toHaveBeenCalledOnce();
    expect(mocks.open).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "配置 快捷导出" }));
    expect(await screen.findByRole("button", { name: "保存组件信息" })).toBeInTheDocument();
  });

  it("runs the selected custom exporter from the footer without refreshing the workspace", async () => {
    const component = { id: "quick", name: "自定义快捷导出", kind: "addon", path: "D:/Project", origin: { kind: "single" }, version: [1, 0, 0], manifests: [], tags: [], favorite: false, size_bytes: 1 };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.settings.mockResolvedValue({ developer_nickname: "MCDH", developer_account: "local", developer_user_id: "0", theme: "dark", quick_export: { destination: "D:/Exports", custom_profile_id: "profile", regenerate_uuids: true, bump_version: true } });
    const task = { id: "quick-task", component_id: "quick", profile_id: "profile", profile_name: "自定义发布", destination: "D:/Exports", status: "running", cancel_requested: false, conflict_path: null, result: null, error: null, logs: [], next_cursor: 0, logs_truncated: false };
    mocks.quickCustomExport.mockResolvedValue(task);
    mocks.customExportTask.mockResolvedValue({ ...task, status: "succeeded", result: { actual_path: "D:/Exports/release.custom", modified_files: [], warnings: [], component: { ...component, version: [1, 0, 1] } }, logs: [{ sequence: 1, source: "stdout", text: "custom release complete" }], next_cursor: 1 });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "一键导出 自定义快捷导出" }));
    expect(await screen.findByText("custom release complete")).toBeInTheDocument();
    expect(screen.getByText("v1.0.1")).toBeInTheDocument();
    expect(mocks.quickCustomExport).toHaveBeenCalledWith("quick", "D:/Exports");
    expect(mocks.quickExport).not.toHaveBeenCalled();
    expect(mocks.refresh).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "配置 自定义快捷导出" })).toBeEnabled();
  });

  it("asks for a missing pack location before retrying a normal export", async () => {
    const component = { id: "bomd", name: "BOMD", kind: "addon", path: "D:/BOMD", origin: { kind: "single" }, manifests: [], tags: [], favorite: false, size_bytes: 1 };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.export.mockRejectedValueOnce({ code: "pack_location_required" }).mockResolvedValue({ actual_path: "D:/Exports/BOMD.zip", modified_files: [], warnings: [] });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "配置 BOMD" }));
    fireEvent.change(screen.getByRole("textbox", { name: /^导出目录/ }), { target: { value: "D:/Exports" } });
    fireEvent.click(screen.getByRole("button", { name: "导出游戏 ZIP" }));
    await waitFor(() => expect(mocks.export).toHaveBeenCalledTimes(2));
    expect(mocks.requestDirectory).toHaveBeenCalledWith(component, "source");
    expect(mocks.open).not.toHaveBeenCalled();
    expect(mocks.export.mock.calls[1][0].destination).toBe("D:/Exports");
    expect(mocks.refresh).toHaveBeenCalledOnce();
  });

  it("retains a newly created card when an older in-flight scan finishes", async () => {
    const existing = { id: "old", name: "旧模组", kind: "addon", path: "D:/Lib/Old", origin: { kind: "library" }, tags: [], manifests: [], favorite: false, size_bytes: 1 };
    const added = { ...existing, id: "new", name: "新模组", path: "D:/Lib/New" };
    const initial = { components: [existing], sources: [{ id: "lib", kind: "library", path: "D:/Lib" }], warnings: [] };
    let complete!: (value: unknown) => void;
    mocks.refresh.mockResolvedValueOnce(initial).mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    mocks.create.mockResolvedValue({ actual_path: added.path, component: added, modified_files: [], warnings: [] });
    render(<App />);
    await screen.findByRole("heading", { name: "旧模组" });
    fireEvent.click(screen.getByRole("button", { name: "刷新组件" }));
    fireEvent.click(screen.getByRole("button", { name: "新建组件" }));
    fireEvent.change(screen.getByRole("textbox", { name: "组件名称" }), { target: { value: "新模组" } });
    fireEvent.click(screen.getByRole("button", { name: "创建组件" }));
    await screen.findByRole("heading", { name: "新模组" });
    await act(async () => complete(initial));
    expect(screen.getByRole("heading", { name: "新模组" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "旧模组" })).toBeInTheDocument();
    expect(mocks.refresh).toHaveBeenCalledTimes(2);
  });

  it("saves one-click export preferences with the selected external exporter", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.setSettings.mockImplementation(async (value) => value);
    mocks.customExportProfiles.mockResolvedValue([{ id: "profile", name: "自定义发布", enabled: true, input_mode: "snapshot" }]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    await waitFor(() => expect(mocks.sources).toHaveBeenCalled());
    expect(mocks.vscodeStatus).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "一键导出" }));
    await screen.findByRole("option", { name: "自定义发布" });
    fireEvent.change(screen.getByRole("combobox", { name: "导出方式" }), { target: { value: "profile" } });
    fireEvent.change(screen.getByRole("textbox", { name: "导出目录" }), { target: { value: "D:/Custom" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "刷新 Manifest UUID" }));
    fireEvent.change(screen.getByRole("combobox", { name: "一键导出版本提升方式" }), { target: { value: "minor" } });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await waitFor(() => expect(mocks.setSettings).toHaveBeenCalledWith(expect.objectContaining({ quick_export: { destination: "D:/Custom", custom_profile_id: "profile", regenerate_uuids: false, bump_version: true, version_part: "minor", content_mode: "clean", conflict_policy: "rename" } })));
  });

  it("persists the MCDK switch independently and supports explicit manual installation", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.checkMcdkUpdate.mockResolvedValue({ ...mcdkReady, auto_update: false, phase: "available", latest_version: "1.7.0" });
    mocks.installMcdkUpdate.mockResolvedValue({ ...mcdkReady, auto_update: false, phase: "updated", current_version: "1.7.0" });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /设置/ }));
    fireEvent.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "开发工具" }));
    const toggle = await screen.findByRole("switch", { name: "MCDK 自动更新" });
    await waitFor(() => expect(toggle).toBeEnabled());
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(mocks.setMcdkAutoUpdate).toHaveBeenCalledWith(false);
    expect(mocks.setSettings).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "检查更新" }));
    fireEvent.click(await screen.findByRole("button", { name: "立即更新" }));
    await waitFor(() => expect(mocks.installMcdkUpdate).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("v1.7.0")).toBeInTheDocument();
  });

  it("persists the plugin directory independently, retains it on reopening, and resets to the bundled default", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    const custom = { ...mcdkReady, mcde: { ...mcdkReady.mcde, directory: "D:/Plugins/MCDE", configured: true, host_revision: "def4567", plugin_version: "0.2.0" } };
    mocks.setMcdeDirectory.mockResolvedValueOnce(custom).mockResolvedValueOnce(mcdkReady);
    let choose!: (value: string) => void;
    mocks.open.mockReturnValue(new Promise((resolve) => { choose = resolve; }));
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "开发工具" }));
    const directory = await screen.findByRole("textbox", { name: "MCDE 插件目录" });
    await waitFor(() => expect(directory).toHaveValue("D:/Bundled/MCDE"));
    expect(directory).toHaveAttribute("readonly");
    expect(screen.getByText("宿主分支 / 修订：feature/plugin-system · abc1234")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "恢复内置 MCDE 插件" })).toBeDisabled();
    const picker = screen.getByRole("button", { name: "选择 MCDE 插件目录" });
    fireEvent.click(picker);
    fireEvent.click(picker);
    expect(mocks.open).toHaveBeenCalledExactlyOnceWith({ title: "选择 MCDE 插件目录", directory: true, multiple: false, defaultPath: "D:/Bundled/MCDE" });
    expect(picker).toBeDisabled();
    expect(picker).toHaveAttribute("aria-busy", "true");
    await act(async () => choose("D:/Plugins/MCDE"));
    await waitFor(() => expect(directory).toHaveValue("D:/Plugins/MCDE"));
    expect(mocks.setMcdeDirectory).toHaveBeenCalledExactlyOnceWith("D:/Plugins/MCDE");
    expect(screen.getByText("自定义")).toBeInTheDocument();
    expect(screen.getByText("宿主分支 / 修订：feature/plugin-system · def4567")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "MCDE" })).getByText("v0.2.0")).toBeInTheDocument();
    expect(mocks.setSettings).not.toHaveBeenCalled();
    expect(mocks.setMcdkAutoUpdate).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("dialog", { name: "设置" })).getByText("关闭", { selector: "button" }));
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "开发工具" }));
    expect(screen.getByRole("textbox", { name: "MCDE 插件目录" })).toHaveValue("D:/Plugins/MCDE");
    fireEvent.click(screen.getByRole("button", { name: "恢复内置 MCDE 插件" }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "MCDE 插件目录" })).toHaveValue("D:/Bundled/MCDE"));
    expect(mocks.setMcdeDirectory).toHaveBeenLastCalledWith(null);
    expect(mocks.setMcdeDirectory).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "恢复内置 MCDE 插件" })).toBeDisabled();
    expect(screen.getByText("内置默认")).toBeInTheDocument();
    expect(mocks.setSettings).not.toHaveBeenCalled();
  });

  it("leaves the plugin configuration unchanged when its directory picker is cancelled", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.open.mockResolvedValue(null);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "开发工具" }));
    const picker = await screen.findByRole("button", { name: "选择 MCDE 插件目录" });
    await waitFor(() => expect(picker).toBeEnabled());
    fireEvent.click(picker);
    await waitFor(() => expect(picker).toBeEnabled());
    expect(mocks.open).toHaveBeenCalledOnce();
    expect(mocks.setMcdeDirectory).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox", { name: "MCDE 插件目录" })).toHaveValue("D:/Bundled/MCDE");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it.each(["picker", "validation"] as const)("shows a plugin directory %s error and unlocks the controls", async (failure) => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    if (failure === "picker") mocks.open.mockRejectedValue("无法打开 MCDE 目录选择器");
    else {
      mocks.open.mockResolvedValue("D:/Invalid/MCDE");
      mocks.setMcdeDirectory.mockRejectedValue("MCDE 插件目录校验失败");
    }
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "开发工具" }));
    const picker = await screen.findByRole("button", { name: "选择 MCDE 插件目录" });
    await waitFor(() => expect(picker).toBeEnabled());
    fireEvent.click(picker);
    expect(await screen.findByRole("alert")).toHaveTextContent(failure === "picker" ? "无法打开 MCDE 目录选择器" : "MCDE 插件目录校验失败");
    await waitFor(() => expect(picker).toBeEnabled());
    expect(screen.getByRole("textbox", { name: "MCDE 插件目录" })).toHaveValue("D:/Bundled/MCDE");
    expect(mocks.setSettings).not.toHaveBeenCalled();
    if (failure === "picker") expect(mocks.setMcdeDirectory).not.toHaveBeenCalled();
    else expect(mocks.setMcdeDirectory).toHaveBeenCalledExactlyOnceWith("D:/Invalid/MCDE");
  });

  it.each([undefined, "game", "editor"] as const)("restores a %s session and protects component controls, including legacy sessions", async (mode) => {
    mocks.refresh.mockResolvedValue({ components: [{ id: "game", name: "运行模组", kind: "addon", path: "D:\\作品", origin: { kind: "single" }, manifests: [], tags: [], size_bytes: 1 }], sources: [], warnings: [] });
    mocks.mcdkStatus.mockResolvedValue({ ...mcdkReady, session: { component_id: "game", pid: 100, version: "1.6.1", ...(mode ? { mode, launch_directory: "D:/Project" } : {}) } });
    render(<App />);
    const launch = await screen.findByRole("button", { name: "启动游戏 运行模组" });
    await waitFor(() => expect(launch).toHaveAttribute("title", mode === "editor" ? "MCDE 宿主运行中" : "MCDK 会话运行中"));
    expect(launch).toBeDisabled();
    expect(screen.getByRole("button", { name: "在 MCDE 中打开 运行模组" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "在 MCDE 中打开 运行模组" }));
    fireEvent.click(launch);
    expect(mocks.launchGame).not.toHaveBeenCalled();
    expect(mocks.launchEditor).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "配置 运行模组" }));
    for (const name of ["删除", "移动", "随机重生", "提升版本"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByRole("button", { name: "复制" })).toBeEnabled();
  });

  it("preserves a legacy session against an older status read and launch callbacks", async () => {
    let complete!: (status: McdkStatus) => void;
    let receive!: (status: McdkStatus) => void;
    mocks.mcdkStatus.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    mocks.onMcdkStatus.mockImplementation(async (handler) => { receive = handler; return () => undefined; });
    const notice = vi.fn();
    const { result } = renderHook(() => useMcdk(notice));
    await waitFor(() => expect(mocks.mcdkStatus).toHaveBeenCalledOnce());
    act(() => receive(mcdkReady));
    const launch = result.current.launch;
    const launchEditor = result.current.launchEditor;
    const legacy: McdkSession = { component_id: "game", component_path: "D:/Project", executable: "D:/MCDK.exe", version: "1.6.1", pid: 100, created_at: "2026-10-06T12:00:00Z" };
    act(() => {
      receive({ ...mcdkReady, session: legacy });
      void launch("game");
      void launchEditor("game");
    });
    await act(async () => complete(mcdkReady));
    expect(result.current.status?.session).toBe(legacy);
    expect(mocks.launchGame).not.toHaveBeenCalled();
    expect(mocks.launchEditor).not.toHaveBeenCalled();
    expect(notice).not.toHaveBeenCalled();
  });

  it("does not replace a newer running session with an older configuration response", async () => {
    let complete!: (status: McdkStatus) => void;
    let receive!: (status: McdkStatus) => void;
    mocks.onMcdkStatus.mockImplementation(async (handler) => { receive = handler; return () => undefined; });
    mocks.setMcdeDirectory.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    const notice = vi.fn();
    const { result } = renderHook(() => useMcdk(notice));
    await waitFor(() => expect(result.current.status).toBe(mcdkReady));
    let update!: Promise<void>;
    act(() => { update = result.current.update(() => mocks.setMcdeDirectory("D:/Plugins/MCDE")); });
    const running = { ...mcdkReady, session: editorSession, mcde: { ...mcdkReady.mcde, directory: "D:/Plugins/MCDE", configured: true } };
    act(() => receive(running));
    mocks.mcdkStatus.mockResolvedValue(running);
    await act(async () => { complete({ ...running, session: null }); await update; });
    expect(result.current.status?.session).toBe(editorSession);
    expect(result.current.status?.mcde.directory).toBe("D:/Plugins/MCDE");
    expect(mocks.mcdkStatus).toHaveBeenCalledTimes(2);
  });

  it("reports launcher failures and cleans up the status subscription", async () => {
    mocks.refresh.mockResolvedValue({ components: [{ id: "game", name: "失败测试", kind: "addon", path: "D:\\作品", origin: { kind: "single" }, manifests: [], tags: [], size_bytes: 1 }], sources: [], warnings: [] });
    const stop = vi.fn();
    let receive!: (status: McdkStatus) => void;
    mocks.onMcdkStatus.mockImplementation(async (callback) => { receive = callback; return stop; });
    mocks.launchGame.mockRejectedValue("游戏启动路径不可用");
    const view = render(<App />);
    const button = await screen.findByRole("button", { name: "启动游戏 失败测试" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    expect(await screen.findByText("游戏启动路径不可用")).toBeInTheDocument();
    act(() => receive({ ...mcdkReady, last_exit: { id: "100:created", component_id: "game", exit_code: 7 } }));
    expect(await screen.findByText(/MCDK 启动器异常退出（代码 7）/)).toBeInTheDocument();
    view.unmount();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("filters discovered cards by category, search text, and tag", async () => {
    mocks.refresh.mockResolvedValue({
      components: [
        {
          id: "addon-1",
          name: "中文冒险模组",
          kind: "addon",
          path: "D:\\作品\\冒险",
          origin: { kind: "library", source_id: "source" },
          manifests: [],
          tags: ["开发"],
          modified_at: "2026-08-08T12:00:00Z",
          size_bytes: 1024,
        },
        {
          id: "material-1",
          name: "柔和材质",
          kind: "material",
          path: "D:\\作品\\材质",
          origin: { kind: "single", source_id: "single" },
          manifests: [],
          tags: ["发布"],
          modified_at: "2026-08-09T12:00:00Z",
          size_bytes: 2048,
        },
      ],
      sources: [],
      warnings: [],
    });
    render(<App />);

    await waitFor(() => expect(screen.getByText("中文冒险模组")).toBeInTheDocument());
    expect(screen.getByText("柔和材质")).toBeInTheDocument();

    const navigation = within(screen.getByRole("navigation", { name: "组件分类" }));
    fireEvent.click(navigation.getByRole("button", { name: /材质/ }));
    expect(screen.queryByText("中文冒险模组")).not.toBeInTheDocument();
    expect(screen.getByText("柔和材质")).toBeInTheDocument();

    fireEvent.click(navigation.getByRole("button", { name: /全部组件/ }));
    fireEvent.change(screen.getByPlaceholderText("搜索名称或路径"), {
      target: { value: "冒险" },
    });
    expect(screen.getByText("中文冒险模组")).toBeInTheDocument();
    expect(screen.queryByText("柔和材质")).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索名称或路径"), {
      target: { value: "" },
    });
    fireEvent.change(screen.getAllByRole("combobox")[0], {
      target: { value: "发布" },
    });
    expect(screen.getByText("柔和材质")).toBeInTheDocument();
    expect(screen.queryByText("中文冒险模组")).not.toBeInTheDocument();
  });

  it("sorts by modified time by default and switches to name order", async () => {
    mocks.refresh.mockResolvedValue({
      components: [
        { id: "older", name: "Alpha 组件", kind: "addon", path: "D:\\Alpha", origin: { kind: "library" }, manifests: [], tags: [], modified_at: "2026-08-08T12:00:00Z", size_bytes: 1 },
        { id: "newer", name: "Beta 组件", kind: "addon", path: "D:\\Beta", origin: { kind: "library" }, manifests: [], tags: [], modified_at: "2026-08-09T12:00:00Z", size_bytes: 2 },
      ],
      sources: [],
      warnings: [],
    });
    render(<App />);
    await waitFor(() => expect(screen.getByText("Beta 组件")).toBeInTheDocument());
    expect(within(screen.getAllByRole("article")[0]).getByText("Beta 组件")).toBeInTheDocument();

    fireEvent.change(screen.getByRole("combobox", { name: "排序字段" }), { target: { value: "name" } });
    expect(within(screen.getAllByRole("article")[0]).getByText("Alpha 组件")).toBeInTheDocument();
  });

  it("filters favorites and toggles a card without rescanning", async () => {
    const favorite = { id: "favorite", name: "已收藏", kind: "addon" as const, path: "D:\\收藏", origin: { kind: "library" as const }, manifests: [], tags: [], favorite: true, size_bytes: 1 };
    const regular = { id: "regular", name: "普通组件", kind: "map" as const, path: "D:\\普通", origin: { kind: "library" as const }, manifests: [], tags: [], favorite: false, size_bytes: 1 };
    mocks.refresh.mockResolvedValue({ components: [favorite, regular], sources: [], warnings: [] });
    mocks.metadata.mockResolvedValue({ component: { ...regular, favorite: true }, actual_path: regular.path, modified_files: [regular.path + "\\.mcdh.json"], warnings: [] });
    render(<App />);
    await waitFor(() => expect(screen.getByText("普通组件")).toBeInTheDocument());

    const navigation = within(screen.getByRole("navigation", { name: "组件分类" }));
    fireEvent.click(navigation.getByRole("button", { name: /收藏1/ }));
    expect(screen.getByText("已收藏")).toBeInTheDocument();
    expect(screen.queryByText("普通组件")).not.toBeInTheDocument();

    fireEvent.click(navigation.getByRole("button", { name: /全部组件/ }));
    fireEvent.click(screen.getByRole("button", { name: "收藏 普通组件" }));
    await waitFor(() => expect(mocks.metadata).toHaveBeenCalledWith("regular", "普通组件", [], true));
    expect(navigation.getByRole("button", { name: /收藏2/ })).toBeInTheDocument();
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("edits display metadata locally without rescanning", async () => {
    const component = { id: "metadata", name: "旧名称", kind: "material" as const, path: "D:\\元数据", origin: { kind: "library" as const }, manifests: [], tags: ["旧标签"], favorite: false, size_bytes: 1 };
    const updated = { ...component, name: "新名称", tags: ["开发", "测试"], favorite: true };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.metadata.mockResolvedValue({ component: updated, actual_path: component.path, modified_files: [component.path + "\\.mcdh.json"], warnings: [] });
    render(<App />);
    await waitFor(() => expect(screen.getByText("旧名称")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "配置 旧名称" }));
    fireEvent.change(screen.getByRole("textbox", { name: "显示名称" }), { target: { value: "新名称" } });
    fireEvent.change(screen.getByRole("textbox", { name: /标签/ }), { target: { value: "开发, 测试" } });
    fireEvent.click(screen.getByRole("checkbox", { name: /收藏组件/ }));
    fireEvent.click(screen.getByRole("button", { name: "保存组件信息" }));

    await waitFor(() => expect(mocks.metadata).toHaveBeenCalledWith("metadata", "新名称", ["开发", " 测试"], true));
    expect(await screen.findByText("新名称")).toBeInTheDocument();
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("previews a palette and restores saved appearance when settings are dismissed", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.click(within(await screen.findByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "外观" }));
    fireEvent.click(await screen.findByRole("radio", { name: "石墨灰" }));
    fireEvent.click(screen.getByRole("radio", { name: "深色" }));
    expect(document.documentElement.dataset.colorPreset).toBe("graphite");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(mocks.setSettings).not.toHaveBeenCalled();
    fireEvent.click(screen.getAllByRole("button", { name: "关闭" })[0]);
    expect(document.documentElement.dataset.colorPreset).toBe("fluent");
    expect(document.documentElement.dataset.theme).toBe("system");
  });

  it("persists a selected palette and keeps it after reopening settings", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.setSettings.mockImplementation(async (settings) => { mocks.settings.mockResolvedValue(settings); return settings; });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.click(within(await screen.findByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "外观" }));
    fireEvent.click(await screen.findByRole("radio", { name: "macOS 蓝" }));
    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await waitFor(() => expect(mocks.setSettings).toHaveBeenCalledWith(expect.objectContaining({ color_preset: "cupertino", theme: "light", developer_nickname: "MCDH" })));
    await screen.findByText("设置已保存。");
    fireEvent.click(screen.getAllByRole("button", { name: "关闭" })[0]);
    expect(document.documentElement.dataset.colorPreset).toBe("cupertino");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(within(await screen.findByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "外观" }));
    expect(await screen.findByRole("radio", { name: "macOS 蓝" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "浅色" })).toBeChecked();
  });

  it("does not change saved appearance when saving a preview fails", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.setSettings.mockRejectedValue(new Error("write failed"));
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "设置" }));
    fireEvent.click(within(await screen.findByRole("navigation", { name: "设置分类" })).getByRole("button", { name: "外观" }));
    fireEvent.click(await screen.findByRole("radio", { name: "鸢尾紫" }));
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await screen.findByText("Error: write failed");
    fireEvent.click(screen.getAllByRole("button", { name: "关闭" })[0]);
    expect(document.documentElement.dataset.colorPreset).toBe("fluent");
  });

  it("does not rescan when the window regains focus", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    render(<App />);
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1));

    window.dispatchEvent(new Event("focus"));
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("uses configured destinations and reveals the MCS namespace", async () => {
    mocks.refresh.mockResolvedValue({
      components: [],
      sources: [
        { id: "library", kind: "library", path: "D:\\作品库" },
        { id: "addon", kind: "mcs_auto", path: "D:\\work\\account\\Cpp\\AddOn" },
        { id: "map", kind: "mcs_auto", path: "D:\\work\\account\\Cpp\\Map" },
      ],
      warnings: [],
    });
    mocks.settings.mockResolvedValue({ developer_nickname: "MCDH", developer_account: "local", developer_user_id: "0", default_destination: "D:\\作品库", theme: "system" });
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: /新建组件/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /新建组件/ }));
    expect(screen.getByLabelText("生成位置")).toHaveValue("D:\\作品库");

    fireEvent.click(screen.getByRole("checkbox", { name: /生成 MCS 兼容配置/ }));
    expect(screen.getByLabelText("命名空间")).toHaveValue("mcdh");
    expect(screen.getByLabelText("生成位置")).toHaveValue("D:\\work\\account\\Cpp\\AddOn");
    expect(screen.queryByRole("option", { name: /Cpp\\Map/ })).not.toBeInTheDocument();
  });

  it("sends the explicit full restore mode during import", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.import.mockResolvedValue({ actual_path: "D:\\组件库\\完整恢复", modified_files: [], warnings: [] });
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: "导入" })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "导入" }));
    fireEvent.change(screen.getByPlaceholderText("选择组件包或文件夹"), { target: { value: "D:\\备份\\组件.zip" } });
    fireEvent.change(screen.getByPlaceholderText("选择一个可写目录"), { target: { value: "D:\\组件库" } });
    fireEvent.click(screen.getByRole("checkbox", { name: /完整恢复/ }));
    fireEvent.click(screen.getByRole("button", { name: "开始导入" }));

    await waitFor(() => expect(mocks.import).toHaveBeenCalledWith({
      source: "D:\\备份\\组件.zip",
      destination: "D:\\组件库",
      mcs_compatible: false,
      identity_policy: "error",
      content_mode: "full",
    }));
  });

  it("organizes settings and supports a manual check after the startup check", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.checkForUpdates
      .mockResolvedValueOnce({
        current_version: "1.1.0",
        latest_version: "v1.1.0",
        release_name: "MCDH 1.1.0",
        update_available: false,
        no_release: false,
      })
      .mockResolvedValueOnce({
        current_version: "1.1.0",
        latest_version: "v1.2.0",
        release_name: "MCDH 1.2.0",
        release_url: "https://github.com/xiaobo121388/MCDevHelper/releases/tag/v1.2.0",
        published_at: "2026-08-10T12:00:00Z",
        update_available: true,
        no_release: false,
      });
    render(<App />);
    await waitFor(() => expect(screen.getByRole("button", { name: /设置/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /设置/ }));

    const navigation = await screen.findByRole("navigation", { name: "设置分类" });
    expect(within(navigation).getByRole("button", { name: /路径管理/ })).toHaveAttribute("aria-current", "page");
    await waitFor(() => expect(mocks.checkForUpdates).toHaveBeenCalledTimes(1));
    fireEvent.click(within(navigation).getByRole("button", { name: /关于/ }));
    expect(await screen.findByText("v1.1.0")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "检查更新" }));
    await waitFor(() => expect(mocks.checkForUpdates).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("发现新版本 v1.2.0")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /查看 Release/ }));
    await waitFor(() => expect(mocks.openUrl).toHaveBeenCalledWith("https://github.com/xiaobo121388/MCDevHelper/releases/tag/v1.2.0"));

    fireEvent.click(screen.getByRole("button", { name: /打开反馈页面/ }));
    await waitFor(() => expect(mocks.openUrl).toHaveBeenCalledWith("https://github.com/xiaobo121388/MCDevHelper/issues/new"));
    fireEvent.click(screen.getByRole("button", { name: "立即更新" }));
    await waitFor(() => expect(mocks.installAppUpdate).toHaveBeenCalledWith("v1.2.0", expect.any(Function)));
  });

  it("automatically reports a newer GitHub release on startup", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.checkForUpdates.mockResolvedValue({
      current_version: "1.1.0",
      latest_version: "v1.2.0",
      release_name: "MCDH 1.2.0",
      release_url: "https://github.com/xiaobo121388/MCDevHelper/releases/tag/v1.2.0",
      release_notes: "新增批量管理功能\n修复已知问题",
      published_at: "2026-08-12T12:00:00Z",
      update_available: true,
      no_release: false,
    });

    render(<App />);
    expect(await screen.findByRole("heading", { name: "发现新版本 v1.2.0" })).toBeInTheDocument();
    expect(screen.getByText(/新增批量管理功能/)).toBeInTheDocument();
    expect(mocks.checkForUpdates).toHaveBeenCalledTimes(1);

    expect(mocks.installAppUpdate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "立即更新" }));
    await waitFor(() => expect(mocks.installAppUpdate).toHaveBeenCalledWith("v1.2.0", expect.any(Function)));
    expect(mocks.openUrl).not.toHaveBeenCalled();
    expect(await screen.findByRole("heading", { name: "正在安装，即将自动重启" })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("heading", { name: "发现新版本 v1.2.0" })).not.toBeInTheDocument());
  });

  it("shows the bundled changelog once after upgrading, before another update prompt", async () => {
    window.localStorage.setItem("mcdh.last-launched-version", "1.0.0");
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.checkForUpdates.mockResolvedValue({
      current_version: "1.1.0",
      latest_version: "v1.2.0",
      release_name: "MCDH 1.2.0",
      release_url: "https://github.com/xiaobo121388/MCDevHelper/releases/tag/v1.2.0",
      update_available: true,
      no_release: false,
    });

    const { unmount } = render(<App />);
    expect(await screen.findByRole("heading", { name: "已更新至 v1.1.0" })).toBeInTheDocument();
    expect(screen.getByText(/新增 .mcdh.json 可携带组件元数据/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "发现新版本 v1.2.0" })).not.toBeInTheDocument();
    expect(window.localStorage.getItem("mcdh.last-launched-version")).toBe("1.1.0");

    fireEvent.click(screen.getByRole("button", { name: "知道了" }));
    expect(await screen.findByRole("heading", { name: "发现新版本 v1.2.0" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "稍后提醒" }));
    unmount();

    mocks.checkForUpdates.mockResolvedValue({
      current_version: "1.1.0",
      latest_version: "v1.1.0",
      update_available: false,
      no_release: false,
    });
    render(<App />);
    await waitFor(() => expect(mocks.checkForUpdates).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("heading", { name: "已更新至 v1.1.0" })).not.toBeInTheDocument();
  });

  it("shows the Git-based 1.3.0 changelog once when upgrading from 1.2.0", async () => {
    window.localStorage.setItem("mcdh.last-launched-version", "1.2.0");
    mocks.version.mockResolvedValue("1.3.0");
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.checkForUpdates.mockResolvedValue({ current_version: "1.3.0", latest_version: "v1.2.0", update_available: false, no_release: false });
    const view = render(<App />);
    expect(await screen.findByRole("heading", { name: "已更新至 v1.3.0" })).toBeInTheDocument();
    for (const note of releaseNotesFor("1.3.0")) expect(screen.getByText(note)).toBeInTheDocument();
    expect(window.localStorage.getItem("mcdh.last-launched-version")).toBe("1.3.0");
    fireEvent.click(screen.getByRole("button", { name: "知道了" }));
    view.unmount();
    render(<App />);
    await waitFor(() => expect(mocks.checkForUpdates).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("heading", { name: "已更新至 v1.3.0" })).not.toBeInTheDocument();
  });

  it("shows download progress, prevents duplicate installs, and permits retry after a failure", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.checkForUpdates.mockResolvedValue({ current_version: "1.1.0", latest_version: "v1.2.0", update_available: true, no_release: false });
    let report!: (progress: AppUpdateProgress) => void;
    let fail!: (reason: Error) => void;
    mocks.installAppUpdate.mockImplementationOnce((_version, handler) => {
      report = handler;
      return new Promise((_resolve, reject) => { fail = reject; });
    });
    render(<App />);
    const install = await screen.findByRole("button", { name: "立即更新" });
    fireEvent.click(install);
    fireEvent.click(install);
    expect(mocks.installAppUpdate).toHaveBeenCalledTimes(1);
    act(() => report({ phase: "downloading", downloaded_bytes: 40, total_bytes: 100 }));
    expect(screen.getByRole("heading", { name: "正在下载更新" })).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "更新下载进度" })).toHaveAttribute("value", "40");
    expect(screen.queryByRole("button", { name: "立即更新" })).not.toBeInTheDocument();
    await act(async () => fail(new Error("SHA-256 校验失败")));
    expect(await screen.findByRole("alert")).toHaveTextContent("SHA-256 校验失败");
    fireEvent.click(screen.getByRole("button", { name: "立即更新" }));
    await waitFor(() => expect(mocks.installAppUpdate).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("heading", { name: "正在安装，即将自动重启" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(mocks.openUrl).not.toHaveBeenCalled();
  });

  it("persists installation failure feedback until explicitly dismissed", async () => {
    mocks.refresh.mockResolvedValue({ components: [], sources: [], warnings: [] });
    mocks.appUpdateError.mockResolvedValue("Silent installer failed; original application files restored.");
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("original application files restored");
    fireEvent.click(within(screen.getByRole("alert")).getByRole("button", { name: "知道了" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(mocks.appUpdateError).toHaveBeenLastCalledWith(true);
  });

  it("requires two confirmations before deleting a component", async () => {
    mocks.refresh.mockResolvedValue({
      components: [{ id: "delete-me", name: "待删除", kind: "addon", path: "D:\\待删除", origin: { kind: "library" }, manifests: [], tags: [], size_bytes: 1 }],
      sources: [],
      warnings: [],
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);
    await waitFor(() => expect(screen.getByText("待删除")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "配置 待删除" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    await waitFor(() => expect(mocks.delete).toHaveBeenCalledWith("delete-me"));
    expect(confirm).toHaveBeenCalledTimes(2);
    confirm.mockRestore();
  });

  it("keeps component configuration open when reopening after a successful custom export", async () => {
    const component = { id: "exported", name: "导出后的模组", kind: "addon", path: "D:\\项目", origin: { kind: "single" }, manifests: [], tags: [], favorite: false, size_bytes: 1 };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.customExportProfiles.mockResolvedValue([{ id: "packer", name: "测试打包", enabled: true, component_kinds: ["addon"], input_mode: "snapshot" }]);
    const succeeded = {
      id: "completed-export", component_id: component.id, profile_id: "packer", profile_name: "测试打包", destination: "D:\\导出",
      status: "succeeded", cancel_requested: false, conflict_path: null, error: null,
      result: { actual_path: "D:\\导出\\test.zip", modified_files: [], warnings: [] },
      logs: [{ sequence: 1, source: "stdout", text: "历史打包日志" }], next_cursor: 1, logs_truncated: false,
    };
    mocks.startCustomExport.mockResolvedValue({ ...succeeded, status: "preparing", result: null, logs: [], next_cursor: 0 });
    mocks.customExportTask.mockResolvedValue(succeeded);
    window.localStorage.setItem("mcdh.last-export-destination", "D:\\导出");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "配置 导出后的模组" }));
    fireEvent.click(await screen.findByRole("button", { name: "测试打包" }));
    expect(await screen.findByText("自定义导出已完成")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    mocks.customExportTasks.mockResolvedValue([{ ...succeeded, logs: [] }]);

    for (let attempt = 0; attempt < 2; attempt++) {
      fireEvent.click(screen.getByRole("button", { name: "配置 导出后的模组" }));
      expect(await screen.findByText("历史打包日志")).toBeInTheDocument();
      expect(screen.getByRole("dialog", { name: "导出后的模组" })).toBeInTheDocument();
      expect(screen.getByLabelText("显示名称")).toHaveValue(component.name);
      expect(screen.getByRole("button", { name: "测试打包" })).toBeEnabled();
      fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "关闭" }));
    }
    expect(mocks.startCustomExport).toHaveBeenCalledTimes(1);
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("updates UUID and version results without rescanning every source", async () => {
    const component = {
      id: "fast-update",
      name: "快速配置模组",
      kind: "addon" as const,
      path: "D:\\快速配置",
      origin: { kind: "library" as const, source_id: "library" },
      manifests: [],
      version: [1, 0, 0] as [number, number, number],
      tags: [],
      size_bytes: 16,
    };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.regenerateUuids.mockResolvedValue({
      component,
      actual_path: component.path,
      modified_files: [component.path + "\\manifest.json"],
      warnings: [],
    });
    const bumped = { ...component, version: [1, 0, 1] as [number, number, number] };
    mocks.bumpVersion.mockResolvedValue({
      component: bumped,
      actual_path: component.path,
      modified_files: [component.path + "\\manifest.json"],
      warnings: [],
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);
    await waitFor(() => expect(screen.getByText("快速配置模组")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "配置 快速配置模组" }));
    fireEvent.click(screen.getByRole("button", { name: "随机重生" }));
    await waitFor(() => expect(mocks.regenerateUuids).toHaveBeenCalledWith(component.id));
    expect(confirm).not.toHaveBeenCalled();
    expect(mocks.refresh).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "配置 快速配置模组" }));
    fireEvent.click(screen.getByRole("button", { name: "提升版本" }));
    await waitFor(() => expect(mocks.bumpVersion).toHaveBeenCalledWith(component.id, "patch"));
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
    expect(screen.getByText("v1.0.1")).toBeInTheDocument();
    confirm.mockRestore();
  });

  it("keeps the UI responsive and skips rescanning while exporting", async () => {
    const component = {
      id: "large-addon",
      name: "大型模组",
      kind: "addon" as const,
      path: "D:\\大型模组",
      origin: { kind: "library" as const, source_id: "library" },
      manifests: [],
      tags: [],
      size_bytes: 32_000_000,
    };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    let finishExport: (value: unknown) => void = () => undefined;
    mocks.export.mockReturnValue(new Promise((resolve) => { finishExport = resolve; }));
    render(<App />);
    await waitFor(() => expect(screen.getByText("大型模组")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "配置 大型模组" }));
    fireEvent.change(screen.getByLabelText("导出目录"), {
      target: { value: "D:\\导出" },
    });
    fireEvent.click(screen.getByRole("button", { name: "导出游戏 ZIP" }));
    await waitFor(() => expect(mocks.export).toHaveBeenCalledWith({
      component_id: component.id,
      destination: "D:\\导出",
      content_mode: "clean",
      conflict_policy: "error",
    }));
    expect(screen.getByRole("button", { name: "导出中…" })).toBeDisabled();

    finishExport({
      actual_path: "D:\\导出\\大型模组.zip",
      modified_files: ["D:\\导出\\大型模组.zip"],
      warnings: [],
    });
    await waitFor(() => expect(screen.getByText("游戏 ZIP 已导出")).toBeInTheDocument());
    expect(mocks.refresh).toHaveBeenCalledTimes(1);

    mocks.export.mockResolvedValue({
      actual_path: "D:\\导出\\大型模组 完整.zip",
      modified_files: ["D:\\导出\\大型模组 完整.zip"],
      warnings: [],
    });
    fireEvent.click(screen.getByRole("button", { name: "配置 大型模组" }));
    expect(screen.getByLabelText("导出目录")).toHaveValue("D:\\导出");
    fireEvent.click(screen.getByRole("button", { name: "导出完整 ZIP" }));
    await waitFor(() => expect(mocks.export).toHaveBeenLastCalledWith({
      component_id: component.id,
      destination: "D:\\导出",
      content_mode: "full",
      conflict_policy: "error",
    }));
    expect(await screen.findByText("完整 ZIP 已导出")).toBeInTheDocument();
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["添加后缀", "rename"],
    ["覆盖原文件", "overwrite"],
  ] as const)("offers %s when an export file already exists", async (action, policy) => {
    const component = {
      id: `conflict-${policy}`,
      name: "重名模组",
      kind: "addon" as const,
      path: "D:\\重名模组",
      origin: { kind: "library" as const, source_id: "library" },
      manifests: [],
      tags: [],
      size_bytes: 1024,
    };
    mocks.refresh.mockResolvedValue({ components: [component], sources: [], warnings: [] });
    mocks.export
      .mockRejectedValueOnce({ code: "destination_exists", message: "导出文件已存在", path: "D:\\导出\\重名模组.zip" })
      .mockResolvedValueOnce({ actual_path: `D:\\导出\\重名模组${policy === "rename" ? " (2)" : ""}.zip`, modified_files: [], warnings: [] });
    render(<App />);
    await waitFor(() => expect(screen.getByText("重名模组")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "配置 重名模组" }));
    fireEvent.change(screen.getByLabelText("导出目录"), { target: { value: "D:\\导出" } });
    fireEvent.click(screen.getByRole("button", { name: "导出游戏 ZIP" }));
    expect(await screen.findByText("导出文件已存在")).toBeInTheDocument();
    expect(screen.getByText("D:\\导出\\重名模组.zip")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加后缀" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "覆盖原文件" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: action }));
    await waitFor(() => expect(mocks.export).toHaveBeenLastCalledWith({
      component_id: component.id,
      destination: "D:\\导出",
      content_mode: "clean",
      conflict_policy: policy,
    }));
    expect(await screen.findByText("游戏 ZIP 已导出")).toBeInTheDocument();
    expect(window.localStorage.getItem("mcdh.last-export-destination")).toBe("D:\\导出");
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("opens scan problem details and supports opening, ignoring, and removing a source", async () => {
    const problem = {
      path: "D:\\失效作品库\\损坏组件\\manifest.json",
      message: "JSON 解析失败",
    };
    mocks.refresh
      .mockResolvedValueOnce({
        components: [],
        sources: [{ id: "broken-source", kind: "library", path: "D:\\失效作品库" }],
        warnings: [problem],
      })
      .mockResolvedValue({ components: [], sources: [], warnings: [] });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);

    const warningButton = await screen.findByRole("button", { name: /有 1 个扫描问题/ });
    fireEvent.click(warningButton);
    expect(screen.getByRole("heading", { name: "扫描问题" })).toBeInTheDocument();
    expect(screen.getByText("JSON 解析失败")).toBeInTheDocument();
    expect(screen.getByText(problem.path)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "打开文件夹" }));
    await waitFor(() => expect(mocks.openWarningDirectory).toHaveBeenCalledWith(problem.path));

    fireEvent.click(screen.getByRole("button", { name: "忽略" }));
    expect(screen.queryByRole("button", { name: /有 1 个扫描问题/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /已忽略 1 个问题/ })).toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem("mcdh.ignored-discovery-warnings") ?? "[]")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "重新显示" }));
    expect(screen.getByRole("button", { name: /有 1 个扫描问题/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "移除来源" }));
    await waitFor(() => expect(mocks.removeSource).toHaveBeenCalledWith("broken-source"));
    await waitFor(() => expect(screen.getByText("当前没有扫描问题")).toBeInTheDocument());
    expect(confirm).toHaveBeenCalledTimes(1);
    confirm.mockRestore();
  });
});
