import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  refresh: vi.fn(),
  settings: vi.fn(),
  setSettings: vi.fn(),
  delete: vi.fn(),
  regenerateUuids: vi.fn(),
  bumpVersion: vi.fn(),
  openWarningDirectory: vi.fn(),
  removeSource: vi.fn(),
}));

vi.mock("./api", () => ({
  desktop: true,
  api: {
    refresh: mocks.refresh,
    settings: mocks.settings,
    setSettings: mocks.setSettings,
    delete: mocks.delete,
    regenerateUuids: mocks.regenerateUuids,
    bumpVersion: mocks.bumpVersion,
    openWarningDirectory: mocks.openWarningDirectory,
    removeSource: mocks.removeSource,
  },
  errorMessage: (error: unknown) => String(error),
}));

import { App } from "./App";

describe("component workspace filters", () => {
  afterEach(cleanup);

  beforeEach(() => {
    window.localStorage.clear();
    mocks.refresh.mockReset();
    mocks.settings.mockReset().mockResolvedValue({
      developer_nickname: "MCDH",
      developer_account: "mcdh@local.invalid",
      developer_user_id: "0",
      theme: "system",
    });
    mocks.setSettings.mockReset();
    mocks.delete.mockReset().mockResolvedValue({ actual_path: "", modified_files: [], warnings: [] });
    mocks.regenerateUuids.mockReset();
    mocks.bumpVersion.mockReset();
    mocks.openWarningDirectory.mockReset().mockResolvedValue(undefined);
    mocks.removeSource.mockReset().mockResolvedValue(true);
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
    expect(mocks.refresh).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "配置 快速配置模组" }));
    fireEvent.click(screen.getByRole("button", { name: "提升版本" }));
    await waitFor(() => expect(mocks.bumpVersion).toHaveBeenCalledWith(component.id, "patch"));
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
    expect(screen.getByText("v1.0.1")).toBeInTheDocument();
    confirm.mockRestore();
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
