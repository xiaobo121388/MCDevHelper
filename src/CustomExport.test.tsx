import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { CustomExportSettings } from "./CustomExportSettings";
import { CustomExportButtons, CustomExportTaskPanel, useCustomExport } from "./CustomExport";
import type { ComponentSummary, CustomExportProfile, CustomExportTask } from "./types";

vi.mock("./api", () => ({ api: {
  customExportProfiles: vi.fn(), saveCustomExportProfiles: vi.fn(), customExportTasks: vi.fn(),
  startCustomExport: vi.fn(), customExportTask: vi.fn(), cancelCustomExport: vi.fn(), resolveCustomExportConflict: vi.fn(),
}, errorMessage: (error: unknown) => error && typeof error === "object" && "message" in error ? String(error.message) : String(error) }));

const profile: CustomExportProfile = {
  id: "profile", name: "发布测试包", enabled: true, executable: "D:\\Tools\\python.exe", arguments: ["-u", "pack.py", "{input_dir}", "{output_dir}"],
  input_mode: "snapshot", working_directory: null, component_kinds: ["addon"], timeout_seconds: 1800, log_encoding: "utf8", allow_mcp: false,
};
const component: ComponentSummary = { id: "component", name: "组件", kind: "addon", path: "D:\\component", origin: { kind: "single" }, manifests: [], tags: [], favorite: false, size_bytes: 1 };
const baseTask: CustomExportTask = {
  id: "task", component_id: "component", profile_id: "profile", profile_name: "发布测试包", destination: "D:\\output", status: "running",
  cancel_requested: false, conflict_path: null, result: null, error: null, logs: [], next_cursor: 0, logs_truncated: false,
};
const done = vi.fn();
const notice = vi.fn();
function ExportHarness({ destination = "D:\\output", gameRunning = false }: { destination?: string; gameRunning?: boolean }) {
  const controller = useCustomExport(component, destination, done, notice);
  return <><button disabled={controller.busy}>导出游戏 ZIP</button><button disabled={controller.busy}>导出完整 ZIP</button><CustomExportButtons controller={controller} disabled={false} gameRunning={gameRunning} /><CustomExportTaskPanel controller={controller} /></>;
}

describe("custom export", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(api.customExportProfiles).mockResolvedValue([{ ...profile }]);
    vi.mocked(api.customExportTasks).mockResolvedValue([]);
    vi.mocked(api.saveCustomExportProfiles).mockImplementation(async (items) => items);
    vi.mocked(api.startCustomExport).mockResolvedValue(baseTask);
    vi.mocked(api.customExportTask).mockResolvedValue(baseTask);
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });
  afterEach(() => { cleanup(); vi.restoreAllMocks(); });

  it("adds buttons beside built-ins and respects enabled/type/source constraints", async () => {
    vi.mocked(api.customExportProfiles).mockResolvedValue([
      profile, { ...profile, id: "source", name: "原目录打包", input_mode: "source" },
      { ...profile, id: "off", name: "已停用", enabled: false }, { ...profile, id: "map", name: "地图专用", component_kinds: ["map"] },
    ]);
    render(<ExportHarness gameRunning />);
    expect(await screen.findByRole("button", { name: "发布测试包" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "原目录打包" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导出完整 ZIP" }).nextElementSibling).toHaveTextContent("发布测试包");
    expect(screen.queryByRole("button", { name: "已停用" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "地图专用" })).not.toBeInTheDocument();
  });

  it("requires a destination before running", async () => {
    render(<ExportHarness destination="" />);
    fireEvent.click(await screen.findByRole("button", { name: "发布测试包" }));
    expect(notice).toHaveBeenCalledWith("请先选择导出目录。");
    expect(api.startCustomExport).not.toHaveBeenCalled();
  });

  it("streams logs and resolves conflicts without rerunning the program", async () => {
    const waiting: CustomExportTask = { ...baseTask, status: "awaiting_conflict", conflict_path: "D:\\output\\result.bin", logs: [{ sequence: 1, source: "stdout", text: "packing complete" }], next_cursor: 1 };
    vi.mocked(api.customExportTask).mockResolvedValue(waiting);
    vi.mocked(api.resolveCustomExportConflict).mockResolvedValue({ ...baseTask, status: "publishing" });
    render(<ExportHarness />);
    fireEvent.click(await screen.findByRole("button", { name: "发布测试包" }));
    expect(await screen.findByText("packing complete")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出游戏 ZIP" })).toBeDisabled();
    const result = { actual_path: "D:\\output\\result (2).bin", modified_files: [], warnings: [] };
    vi.mocked(api.customExportTask).mockResolvedValue({ ...baseTask, status: "succeeded", result, next_cursor: 1 });
    fireEvent.click(await screen.findByRole("button", { name: "添加后缀" }));
    await waitFor(() => expect(done).toHaveBeenCalledWith(result, "D:\\output"));
    expect(api.startCustomExport).toHaveBeenCalledTimes(1);
    expect(api.resolveCustomExportConflict).toHaveBeenCalledWith("task", "rename");
    expect(api.customExportTask).toHaveBeenLastCalledWith("task", 1);
    expect(screen.getByRole("button", { name: "导出游戏 ZIP" })).toBeEnabled();
  });

  it("restores active tasks and cancels without losing log output", async () => {
    vi.mocked(api.customExportTasks).mockResolvedValue([baseTask]);
    vi.mocked(api.customExportTask).mockResolvedValue({ ...baseTask, logs: [{ sequence: 1, source: "stderr", text: "working" }], next_cursor: 1 });
    vi.mocked(api.cancelCustomExport).mockResolvedValue({ ...baseTask, cancel_requested: true });
    render(<ExportHarness />);
    expect(await screen.findByText("working")).toBeInTheDocument();
    vi.mocked(api.customExportTask).mockResolvedValue({ ...baseTask, status: "cancelled", next_cursor: 1 });
    fireEvent.click(screen.getByRole("button", { name: "取消任务" }));
    expect(await screen.findByText("已取消")).toBeInTheDocument();
    expect(screen.getByText("working")).toBeInTheDocument();
    expect(api.cancelCustomExport).toHaveBeenCalledWith("task");
    expect(api.startCustomExport).not.toHaveBeenCalled();
  });

  it("shows failed exit code and permits another run", async () => {
    vi.mocked(api.customExportTask).mockResolvedValue({ ...baseTask, status: "failed", error: { code: "execution_failed", message: "打包失败", exit_code: 7 } });
    render(<ExportHarness />);
    fireEvent.click(await screen.findByRole("button", { name: "发布测试包" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("退出码 7");
    expect(screen.getByRole("button", { name: "发布测试包" })).toBeEnabled();
    expect(done).not.toHaveBeenCalled();
  });

  it("edits ordered argument arrays and revokes previous MCP authorization", async () => {
    vi.mocked(api.customExportProfiles).mockResolvedValue([{ ...profile, allow_mcp: true }]);
    render(<CustomExportSettings />);
    const input = await screen.findByLabelText("参数 3");
    fireEvent.change(input, { target: { value: "--input={input_dir}" } });
    expect(screen.getByLabelText("允许 MCP 执行")).not.toBeChecked();
    expect(screen.getByLabelText("允许 MCP 执行")).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "添加参数" }));
    fireEvent.change(screen.getByLabelText("参数 5"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "保存导出方案" }));
    await waitFor(() => expect(api.saveCustomExportProfiles).toHaveBeenCalledWith([{ ...profile, arguments: ["-u", "pack.py", "--input={input_dir}", "{output_dir}", ""] }]));
    expect(await screen.findByText("导出方案已保存。")).toBeInTheDocument();
    expect(screen.getByLabelText("允许 MCP 执行")).toBeEnabled();
  });

  it("adds reorders and removes profiles independently", async () => {
    render(<CustomExportSettings />);
    await screen.findByDisplayValue("发布测试包");
    fireEvent.click(screen.getByRole("button", { name: "新增方案" }));
    fireEvent.change(screen.getByLabelText("按钮名称"), { target: { value: "第二个方案" } });
    fireEvent.change(screen.getByLabelText("程序"), { target: { value: "D:\\pack.exe" } });
    fireEvent.click(screen.getByRole("button", { name: "上移 第二个方案" }));
    fireEvent.click(screen.getByRole("button", { name: "删除 发布测试包" }));
    fireEvent.click(screen.getByRole("button", { name: "保存导出方案" }));
    await waitFor(() => expect(api.saveCustomExportProfiles).toHaveBeenCalled());
    const saved = vi.mocked(api.saveCustomExportProfiles).mock.calls[0][0];
    expect(saved).toHaveLength(1); expect(saved[0].name).toBe("第二个方案");
  });

  it("confirms source mode and never overwrites failed configuration loads", async () => {
    const { unmount } = render(<CustomExportSettings />);
    fireEvent.change(await screen.findByLabelText("输入方式"), { target: { value: "source" } });
    vi.mocked(window.confirm).mockReturnValue(false);
    fireEvent.click(screen.getByRole("button", { name: "保存导出方案" }));
    expect(api.saveCustomExportProfiles).not.toHaveBeenCalled();
    unmount();
    vi.mocked(api.customExportProfiles).mockRejectedValue(new Error("配置损坏"));
    render(<CustomExportSettings />);
    expect(await screen.findByRole("alert")).toHaveTextContent("配置损坏");
    expect(screen.getByRole("button", { name: "保存导出方案" })).toBeDisabled();
  });
});
