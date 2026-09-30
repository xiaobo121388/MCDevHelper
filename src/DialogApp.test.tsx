import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ settings: vi.fn(), sources: vi.fn(), create: vi.fn(), component: vi.fn(), customExportProfiles: vi.fn(), customExportTasks: vi.fn(), notify: vi.fn(), close: vi.fn(), open: vi.fn(), show: vi.fn(), listen: vi.fn(), stop: vi.fn() }));
vi.mock("./api", () => ({ desktop: false, errorMessage: (value: unknown) => String(value), api: { settings: mocks.settings, sources: mocks.sources, create: mocks.create, component: mocks.component, customExportProfiles: mocks.customExportProfiles, customExportTasks: mocks.customExportTasks } }));
vi.mock("./WindowChrome", () => ({ WindowChrome: () => <header>Window chrome</header> }));
vi.mock("./windows", () => ({
  nativeWindows: true, dialogRequest: { kind: "create" }, WORKSPACE_CHANGED: "workspace", SETTINGS_CHANGED: "settings",
  closeDialogWindow: mocks.close, notifyWorkspace: mocks.notify, notifySettings: vi.fn(), openDialogWindow: mocks.open, confirmAction: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen, emitTo: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ show: mocks.show }) }));

import { DialogApp } from "./DialogApp";
import { DEFAULT_SETTINGS } from "./App";
import type { ComponentSummary } from "./types";

const component: ComponentSummary = { id: "test", name: "即时组件", kind: "addon", path: "D:/Project", origin: { kind: "single", source_id: "source" }, manifests: [], tags: ["开发"], favorite: false, size_bytes: 1 };

describe("standalone dialog application", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.settings.mockResolvedValue({ developer_nickname: "MCDH", developer_account: "local", developer_user_id: "0", theme: "dark", color_preset: "graphite" });
    mocks.sources.mockResolvedValue([{ id: "library", kind: "library", path: "D:/TestLibrary" }]);
    mocks.create.mockResolvedValue({ actual_path: "D:/TestLibrary/New" });
    mocks.customExportProfiles.mockResolvedValue([]); mocks.customExportTasks.mockResolvedValue([]);
    mocks.notify.mockResolvedValue(undefined); mocks.close.mockResolvedValue(undefined); mocks.open.mockResolvedValue(undefined);
    mocks.show.mockResolvedValue(undefined); mocks.listen.mockResolvedValue(mocks.stop);
  });
  afterEach(cleanup);

  it("loads native dialog content without a workspace or overlay", async () => {
    const { container } = render(<DialogApp request={{ kind: "create" }} />);
    expect(await screen.findByRole("heading", { name: "新建组件" })).toBeInTheDocument();
    expect(container.querySelector(".standalone-modal")).toBeInTheDocument();
    expect(container.querySelector(".modal-backdrop")).toBeNull();
    expect(container.querySelector(".app-shell")).toBeNull();
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    expect(document.documentElement.dataset.colorPreset).toBe("graphite");
    expect(mocks.show).toHaveBeenCalledOnce();
  });

  it("notifies other windows before closing after a successful operation", async () => {
    render(<DialogApp request={{ kind: "create" }} />);
    fireEvent.change(await screen.findByRole("textbox", { name: "组件名称" }), { target: { value: "New" } });
    fireEvent.click(screen.getByRole("button", { name: "创建组件" }));
    await waitFor(() => expect(mocks.close).toHaveBeenCalledOnce());
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ name: "New", destination: "D:/TestLibrary" }));
    expect(mocks.notify).toHaveBeenCalledWith("已创建到 D:/TestLibrary/New", undefined, true);
    expect(mocks.notify.mock.invocationCallOrder[0]).toBeLessThan(mocks.close.mock.invocationCallOrder[0]);
  });

  it("refreshes available destinations when another window changes the workspace", async () => {
    mocks.sources.mockResolvedValueOnce([]);
    render(<DialogApp request={{ kind: "create" }} />);
    fireEvent.click(await screen.findByRole("button", { name: "前往设置" }));
    expect(mocks.open).toHaveBeenCalledWith({ kind: "settings" });
    expect(mocks.close).not.toHaveBeenCalled();
    const handler = mocks.listen.mock.calls.find(([name]) => name === "workspace")![1];
    await act(async () => handler());
    expect(await screen.findByRole("combobox", { name: "生成位置" })).toHaveValue("D:/TestLibrary");
  });

  it("updates an existing dialog when a saved palette changes in another window", async () => {
    render(<DialogApp request={{ kind: "create" }} />);
    await screen.findByRole("textbox", { name: "组件名称" });
    mocks.settings.mockResolvedValue({ developer_nickname: "MCDH", developer_account: "local", developer_user_id: "0", theme: "light", color_preset: "rose" });
    const handler = mocks.listen.mock.calls.find(([name]) => name === "settings")![1];
    await act(async () => handler());
    await waitFor(() => expect(document.documentElement.dataset.colorPreset).toBe("rose"));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.style.getPropertyValue("--preset-accent-light")).toBe("#b23d70");
  });

  it("keeps load failures visible and supports retry", async () => {
    mocks.settings.mockRejectedValueOnce(new Error("temporary read failure"));
    render(<DialogApp request={{ kind: "create" }} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("temporary read failure");
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("textbox", { name: "组件名称" })).toBeInTheDocument();
  });

  it("renders the card snapshot before settings or component I/O finishes", async () => {
    mocks.settings.mockReturnValue(new Promise(() => {}));
    mocks.component.mockReturnValue(new Promise(() => {}));
    render(<DialogApp request={{ kind: "component", componentId: component.id, initialComponent: component, initialSettings: { ...DEFAULT_SETTINGS, theme: "light", color_preset: "cupertino" } }} />);
    expect(screen.getByRole("textbox", { name: "显示名称" })).toHaveValue("即时组件");
    expect(screen.queryByText("正在加载")).not.toBeInTheDocument();
    expect(mocks.component).toHaveBeenCalledWith("test");
    expect(document.documentElement.dataset.colorPreset).toBe("cupertino");
    expect(mocks.sources).not.toHaveBeenCalled();
  });

  it("starts the component lookup without waiting for settings", () => {
    mocks.settings.mockReturnValue(new Promise(() => {}));
    mocks.component.mockReturnValue(new Promise(() => {}));
    render(<DialogApp request={{ kind: "component", componentId: "test" }} />);
    expect(mocks.component).toHaveBeenCalledWith("test");
    expect(screen.getByText("正在加载")).toBeInTheDocument();
  });

  it("does not seed a different component into the configuration form", async () => {
    mocks.component.mockResolvedValue(component);
    render(<DialogApp request={{ kind: "component", componentId: "test", initialComponent: { ...component, id: "wrong", name: "错误组件" } }} />);
    expect(await screen.findByRole("textbox", { name: "显示名称" })).toHaveValue("即时组件");
    expect(screen.queryByText("错误组件")).not.toBeInTheDocument();
  });

  it("updates appearance without rescanning the configured component", async () => {
    mocks.component.mockResolvedValue(component);
    render(<DialogApp request={{ kind: "component", componentId: "test", initialComponent: component }} />);
    await waitFor(() => expect(mocks.settings).toHaveBeenCalledOnce());
    const handler = mocks.listen.mock.calls.find(([name]) => name === "settings")![1];
    await act(async () => handler());
    expect(mocks.component).toHaveBeenCalledOnce();
  });

  it("fills untouched metadata from the fresh lookup without replacing edits", async () => {
    let complete!: (value: ComponentSummary) => void;
    mocks.component.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    render(<DialogApp request={{ kind: "component", componentId: "test", initialComponent: component }} />);
    fireEvent.change(screen.getByRole("textbox", { name: "显示名称" }), { target: { value: "正在编辑的名称" } });
    await act(async () => complete({ ...component, name: "后台读取的名称", tags: ["最新标签"], favorite: true }));
    expect(screen.getByRole("textbox", { name: "显示名称" })).toHaveValue("正在编辑的名称");
    expect(screen.getByRole("textbox", { name: "标签（使用逗号分隔）" })).toHaveValue("最新标签");
    expect(screen.getByRole("checkbox", { name: /收藏组件/ })).toBeChecked();
  });
});
