import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ info: vi.fn(), source: vi.fn(), component: vi.fn(), destination: vi.fn(), open: vi.fn(), notify: vi.fn(), busy: vi.fn(), notice: vi.fn(), saved: vi.fn(), cancel: vi.fn() }));
vi.mock("./api", () => ({ errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error), api: { exportSource: mocks.info, setExportSource: mocks.source, component: mocks.component, setQuickExportDestination: mocks.destination } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("./windows", () => ({ nativeWindows: false, notifyWorkspace: mocks.notify }));

import { ExportDirectoryForm, ExportSourceEditor } from "./ExportLocation";
import type { ExportDirectoryRequest } from "./windows";
import type { ExportSourceInfo } from "./types";

const component = { id: "bomd", name: "BOMD", path: "D:/BOMD", manifests: [] };
const info: ExportSourceInfo = { path: "D:/BOMD/src", configured: true, valid: true, issue: null };
const request: ExportDirectoryRequest = { kind: "export_directory", purpose: "destination", componentId: "bomd", componentName: "BOMD", projectPath: "D:/BOMD", token: "test", owner: "main" };
const editor = () => render(<ExportSourceEditor componentId="bomd" projectPath="D:/BOMD" onBusy={mocks.busy} onNotice={mocks.notice} />);
const form = (purpose: ExportDirectoryRequest["purpose"] = "destination") => render(<ExportDirectoryForm request={{ ...request, purpose }} onBusy={mocks.busy} onNotice={mocks.notice} onSaved={mocks.saved} onCancel={mocks.cancel} />);

describe("visual export locations", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.info.mockResolvedValue(info);
    mocks.source.mockImplementation(async (_id, path) => path);
    mocks.component.mockResolvedValue(component);
    mocks.destination.mockImplementation(async (destination) => ({ quick_export: { destination } }));
    mocks.notify.mockResolvedValue(undefined);
    mocks.saved.mockResolvedValue(undefined);
    mocks.open.mockResolvedValue("D:/BOMD/assets");
  });
  afterEach(cleanup);

  it("shows saved package locations before any export without opening a system picker", async () => {
    editor();
    expect(await screen.findByText("已配置 · 已识别包体")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "包体位置" })).toHaveValue("D:/BOMD/src");
    expect(screen.getByRole("button", { name: "保存包体位置" })).toBeDisabled();
    expect(mocks.info).toHaveBeenCalledWith("bomd");
    expect(mocks.open).not.toHaveBeenCalled(); expect(mocks.source).not.toHaveBeenCalled();
  });

  it("shows the automatic project root and unresolved packages immediately", async () => {
    mocks.info.mockResolvedValue({ path: "D:/BOMD", configured: false, valid: false, issue: "此目录没有可识别的包体" });
    editor();
    expect(await screen.findByText("未识别到包体")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "包体位置" })).toHaveValue("D:/BOMD");
    expect(screen.getByText("此目录没有可识别的包体")).toBeInTheDocument();
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("does not block the path field or replace edits when lookup finishes", async () => {
    let complete!: (info: ExportSourceInfo) => void;
    mocks.info.mockReturnValue(new Promise((resolve) => { complete = resolve; }));
    editor();
    fireEvent.change(screen.getByRole("textbox", { name: "包体位置" }), { target: { value: "D:/BOMD/edited" } });
    await act(async () => complete(info));
    expect(screen.getByRole("textbox", { name: "包体位置" })).toHaveValue("D:/BOMD/edited");
    expect(screen.getByText("尚未保存")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存包体位置" })).toBeEnabled();
  });

  it("opens Windows only after Browse and saves incrementally without closing configuration", async () => {
    editor();
    await screen.findByText("已配置 · 已识别包体");
    fireEvent.click(screen.getByRole("button", { name: "浏览包体位置" }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "包体位置" })).toHaveValue("D:/BOMD/assets"));
    expect(mocks.open).toHaveBeenCalledWith({ title: "选择包体位置", directory: true, multiple: false, defaultPath: "D:/BOMD/src" });
    expect(mocks.source).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存包体位置" }));
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith("包体位置已保存"));
    expect(mocks.source).toHaveBeenCalledWith("bomd", "D:/BOMD/assets");
    expect(mocks.notify).toHaveBeenCalledWith(undefined, { component, actual_path: "D:/BOMD", modified_files: [], warnings: [] }, false);
    expect(screen.getByRole("textbox", { name: "包体位置" })).toBeInTheDocument();
    expect(mocks.busy.mock.calls).toEqual([[true], [false]]);
  });

  it("keeps invalid source selections in the built-in window and allows correction", async () => {
    mocks.source.mockRejectedValueOnce(new Error("包体目录必须位于当前项目内"));
    form("source");
    await screen.findByText("已配置 · 已识别包体");
    fireEvent.change(screen.getByRole("textbox", { name: "包体位置" }), { target: { value: "D:/Outside" } });
    fireEvent.click(screen.getByRole("button", { name: "保存并继续" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("包体目录必须位于当前项目内");
    expect(mocks.saved).not.toHaveBeenCalled(); expect(mocks.notify).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "包体位置" }), { target: { value: "D:/BOMD/src" } });
    fireEvent.click(screen.getByRole("button", { name: "保存并继续" }));
    await waitFor(() => expect(mocks.saved).toHaveBeenCalledWith("D:/BOMD/src"));
    expect(mocks.source).toHaveBeenCalledTimes(2);
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("keeps source and destination separate and permits typed destination paths", async () => {
    form();
    expect(screen.getByText("D:/BOMD")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "导出目录" })).toHaveValue("");
    expect(mocks.open).not.toHaveBeenCalled(); expect(mocks.info).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "导出目录" }), { target: { value: " D:/Exports " } });
    fireEvent.click(screen.getByRole("button", { name: "保存并继续导出" }));
    await waitFor(() => expect(mocks.saved).toHaveBeenCalledWith("D:/Exports"));
    expect(mocks.destination).toHaveBeenCalledWith("D:/Exports");
    expect(mocks.source).not.toHaveBeenCalled(); expect(mocks.open).not.toHaveBeenCalled();
  });

  it("does not save or export when either built-in window is cancelled", async () => {
    const view = form();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(mocks.cancel).toHaveBeenCalledOnce();
    view.unmount();
    form("source");
    await screen.findByText("已配置 · 已识别包体");
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(mocks.cancel).toHaveBeenCalledTimes(2);
    expect(mocks.source).not.toHaveBeenCalled(); expect(mocks.destination).not.toHaveBeenCalled(); expect(mocks.saved).not.toHaveBeenCalled();
  });

  it("opens the destination system picker only on an explicit click", async () => {
    mocks.open.mockResolvedValue("D:/Exports");
    form();
    fireEvent.click(screen.getByRole("button", { name: "浏览导出目录" }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "导出目录" })).toHaveValue("D:/Exports"));
    expect(mocks.open).toHaveBeenCalledWith({ title: "选择导出目录", directory: true, multiple: false, defaultPath: undefined });
    expect(mocks.destination).not.toHaveBeenCalled();
  });

  it("keeps destination validation errors inline and prevents repeated saves", async () => {
    let fail!: (error: Error) => void;
    mocks.destination.mockReturnValueOnce(new Promise((_resolve, reject) => { fail = reject; }));
    form();
    fireEvent.change(screen.getByRole("textbox", { name: "导出目录" }), { target: { value: "D:/Denied" } });
    const button = screen.getByRole("button", { name: "保存并继续导出" });
    fireEvent.click(button); fireEvent.click(button);
    expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
    expect(mocks.destination).toHaveBeenCalledOnce();
    await act(async () => fail(new Error("目录不可写")));
    expect(await screen.findByRole("alert")).toHaveTextContent("目录不可写");
    expect(mocks.saved).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "导出目录" }), { target: { value: "D:/Exports" } });
    fireEvent.click(button);
    await waitFor(() => expect(mocks.saved).toHaveBeenCalledWith("D:/Exports"));
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("supports retry after a source lookup fails", async () => {
    mocks.info.mockRejectedValueOnce(new Error("temporary read failure"));
    editor();
    expect(await screen.findByRole("alert")).toHaveTextContent("temporary read failure");
    fireEvent.click(screen.getByRole("button", { name: "重新识别" }));
    expect(await screen.findByText("已配置 · 已识别包体")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not replace a synchronization failure with an ordinary success notice", async () => {
    mocks.component.mockRejectedValue(new Error("window sync failed"));
    editor();
    await screen.findByText("已配置 · 已识别包体");
    fireEvent.change(screen.getByRole("textbox", { name: "包体位置" }), { target: { value: "D:/BOMD/assets" } });
    fireEvent.click(screen.getByRole("button", { name: "保存包体位置" }));
    await waitFor(() => expect(mocks.notice).toHaveBeenCalledWith("包体位置已保存，组件同步失败：window sync failed"));
    expect(mocks.notice).toHaveBeenCalledOnce();
    expect(mocks.source).toHaveBeenCalledWith("bomd", "D:/BOMD/assets");
  });
});
