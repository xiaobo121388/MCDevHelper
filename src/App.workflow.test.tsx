import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  refresh: vi.fn(),
  settings: vi.fn(),
  setSettings: vi.fn(),
  delete: vi.fn(),
  export: vi.fn(),
  import: vi.fn(),
  metadata: vi.fn(),
  regenerateUuids: vi.fn(),
  bumpVersion: vi.fn(),
  openWarningDirectory: vi.fn(),
  removeSource: vi.fn(),
  sources: vi.fn(),
  vscodeStatus: vi.fn(),
  version: vi.fn(),
  checkForUpdates: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: mocks.openUrl,
}));

vi.mock("./api", () => ({
  desktop: true,
  api: {
    refresh: mocks.refresh,
    settings: mocks.settings,
    setSettings: mocks.setSettings,
    delete: mocks.delete,
    export: mocks.export,
    import: mocks.import,
    metadata: mocks.metadata,
    regenerateUuids: mocks.regenerateUuids,
    bumpVersion: mocks.bumpVersion,
    openWarningDirectory: mocks.openWarningDirectory,
    removeSource: mocks.removeSource,
    sources: mocks.sources,
    vscodeStatus: mocks.vscodeStatus,
    version: mocks.version,
    checkForUpdates: mocks.checkForUpdates,
  },
  errorMessage: (error: unknown) => String(error),
}));

import { App } from "./App";

describe("component workspace filters", () => {
  afterEach(cleanup);

  beforeEach(() => {
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
    mocks.import.mockReset();
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

    fireEvent.click(screen.getByRole("button", { name: /前往下载/ }));
    await waitFor(() => expect(mocks.openUrl).toHaveBeenCalledWith("https://github.com/xiaobo121388/MCDevHelper/releases/tag/v1.2.0"));
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
