import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ refresh: vi.fn(), settings: vi.fn(), delete: vi.fn() }));

vi.mock("./api", () => ({
  desktop: true,
  api: { refresh: mocks.refresh, settings: mocks.settings, delete: mocks.delete },
  errorMessage: (error: unknown) => String(error),
}));

import { App } from "./App";

describe("component workspace filters", () => {
  afterEach(cleanup);

  beforeEach(() => {
    mocks.refresh.mockReset();
    mocks.settings.mockReset().mockResolvedValue({
      developer_nickname: "MCDH",
      developer_account: "mcdh@local.invalid",
      developer_user_id: "0",
      theme: "system",
    });
    mocks.delete.mockReset().mockResolvedValue({ actual_path: "", modified_files: [], warnings: [] });
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
});
