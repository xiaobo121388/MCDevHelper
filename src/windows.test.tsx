import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(), emit: vi.fn(), listen: vi.fn(), getByLabel: vi.fn(), once: vi.fn(),
  minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(), isMaximized: vi.fn(),
  onResized: vi.fn(), onFocusChanged: vi.fn(), stop: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ emit: mocks.emit, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({
  Window: { getByLabel: mocks.getByLabel },
  getCurrentWindow: () => ({ label: "main", ...mocks }),
}));

import { confirmAction, openDialogWindow } from "./windows";
import { DialogLauncher } from "./DialogLauncher";
import { WindowChrome } from "./WindowChrome";

describe("native window shell", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.invoke.mockResolvedValue("dialog-settings");
    mocks.getByLabel.mockResolvedValue({ once: mocks.once });
    mocks.once.mockResolvedValue(mocks.stop);
    mocks.listen.mockResolvedValue(mocks.stop);
    mocks.emit.mockResolvedValue(undefined);
    mocks.minimize.mockResolvedValue(undefined);
    mocks.toggleMaximize.mockResolvedValue(undefined);
    mocks.close.mockResolvedValue(undefined);
    mocks.isMaximized.mockResolvedValue(false);
    mocks.onResized.mockResolvedValue(mocks.stop);
    mocks.onFocusChanged.mockResolvedValue(mocks.stop);
  });
  afterEach(cleanup);

  it("coalesces simultaneous opens and looks up the native window without creating another", async () => {
    const [first, second] = await Promise.all([openDialogWindow({ kind: "settings" }), openDialogWindow({ kind: "settings" })]);
    expect(first).toBe(second);
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith("open_dialog_window", { request: { kind: "settings" } });
    expect(mocks.getByLabel).toHaveBeenCalledWith("dialog-settings");
    await openDialogWindow({ kind: "settings" });
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
  });

  it("does not render desktop dialogs inside the main document", async () => {
    const closed = vi.fn();
    render(<DialogLauncher request={{ kind: "settings" }} onClose={closed}><div>embedded form</div></DialogLauncher>);
    expect(screen.queryByText("embedded form")).not.toBeInTheDocument();
    await waitFor(() => expect(mocks.once).toHaveBeenCalledWith("tauri://destroyed", expect.any(Function)));
    act(() => mocks.once.mock.calls[0][1]());
    expect(closed).toHaveBeenCalledOnce();
  });

  it("offers retry after a native window fails to open", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("window unavailable"));
    render(<DialogLauncher request={{ kind: "settings" }} onClose={vi.fn()}><div>embedded form</div></DialogLauncher>);
    expect(await screen.findByRole("alert")).toHaveTextContent("window unavailable");
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(mocks.getByLabel).toHaveBeenCalledOnce());
    expect(screen.queryByText("embedded form")).not.toBeInTheDocument();
  });

  it("uses native minimize, maximize and close and follows maximize state", async () => {
    render(<WindowChrome />);
    await waitFor(() => expect(mocks.onResized).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "最小化" }));
    fireEvent.click(screen.getByRole("button", { name: "最大化" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭窗口" }));
    expect(mocks.minimize).toHaveBeenCalledOnce();
    expect(mocks.toggleMaximize).toHaveBeenCalledOnce();
    expect(mocks.close).toHaveBeenCalledOnce();
    mocks.isMaximized.mockResolvedValue(true);
    await act(async () => mocks.onResized.mock.calls[0][0]());
    expect(screen.getByRole("button", { name: "还原窗口" })).toBeInTheDocument();
  });

  it("keeps the close control disabled during an update", () => {
    render(<WindowChrome busy />);
    expect(screen.getByRole("button", { name: "关闭窗口" })).toBeDisabled();
  });

  it("reports native operation errors instead of swallowing them", async () => {
    const report = vi.fn();
    mocks.minimize.mockRejectedValue(new Error("permission denied"));
    render(<WindowChrome onError={report} />);
    fireEvent.click(screen.getByRole("button", { name: "最小化" }));
    await waitFor(() => expect(report).toHaveBeenCalledWith("Error: permission denied"));
  });

  it("resolves a confirmation from its independent window and releases listeners", async () => {
    const answer = confirmAction("continue?");
    await waitFor(() => expect(mocks.once).toHaveBeenCalledOnce());
    expect(mocks.invoke.mock.calls[0][1].request).toMatchObject({ kind: "confirm", message: "continue?", owner: "main" });
    mocks.listen.mock.calls[0][1]({ payload: true });
    expect(await answer).toBe(true);
    expect(mocks.stop).toHaveBeenCalledTimes(2);
  });

  it("cancels a confirmation when its window is closed", async () => {
    const answer = confirmAction("continue?");
    await waitFor(() => expect(mocks.once).toHaveBeenCalledOnce());
    mocks.once.mock.calls[0][1]();
    expect(await answer).toBe(false);
  });

  it("does not approve destructive actions if window creation fails", async () => {
    mocks.invoke.mockRejectedValue(new Error("creation failed"));
    expect(await confirmAction("delete?")).toBe(false);
    expect(mocks.emit).toHaveBeenCalledWith("mcdh:workspace-changed", expect.objectContaining({ message: expect.stringContaining("creation failed") }));
    expect(mocks.stop).toHaveBeenCalledOnce();
  });
});
