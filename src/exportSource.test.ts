import { beforeEach, describe, expect, it, vi } from "vitest";
import { isExportSourceCancelled, withExportSource } from "./exportSource";
import type { ComponentSummary } from "./types";

const mocks = vi.hoisted(() => ({ open: vi.fn(), request: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("./windows", () => ({ requestExportDirectory: mocks.request }));
const component: ComponentSummary = { id: "bomd", name: "BOMD", kind: "addon", path: "D:/BOMD", origin: { kind: "single" }, manifests: [], tags: [], favorite: false, size_bytes: 0 };
const missing = { code: "pack_location_required", path: component.path, message: "请选择包体位置" };

describe("export source selection", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.request.mockResolvedValue("D:/BOMD/src"); });

  it("prompts only after detection fails and retries with the original export arguments", async () => {
    const operation = vi.fn().mockRejectedValueOnce(missing).mockResolvedValue({ actual_path: "D:/Exports/BOMD.zip" });
    expect(await withExportSource(component, operation)).toEqual({ actual_path: "D:/Exports/BOMD.zip" });
    expect(mocks.request).toHaveBeenCalledWith(component, "source");
    expect(mocks.open).not.toHaveBeenCalled();
    expect(operation).toHaveBeenCalledTimes(2);
    expect(mocks.request.mock.invocationCallOrder[0]).toBeLessThan(operation.mock.invocationCallOrder[1]);
  });

  it("does not open a chooser for recognized packages or other failures", async () => {
    await withExportSource(component, vi.fn().mockResolvedValue("ready"));
    const failure = { code: "permission_denied" };
    await expect(withExportSource(component, vi.fn().mockRejectedValue(failure))).rejects.toBe(failure);
    expect(mocks.open).not.toHaveBeenCalled();
    expect(mocks.request).not.toHaveBeenCalled();
  });

  it("cancels without saving a source or retrying the export", async () => {
    mocks.request.mockResolvedValue(null);
    const operation = vi.fn().mockRejectedValue(missing);
    const error = await withExportSource(component, operation).catch(error => error);
    expect(isExportSourceCancelled(error)).toBe(true);
    expect(mocks.open).not.toHaveBeenCalled(); expect(operation).toHaveBeenCalledOnce();
  });

  it("does not retry when the window fails or repeatedly prompt after another failure", async () => {
    mocks.request.mockRejectedValueOnce(new Error("window unavailable"));
    const operation = vi.fn().mockRejectedValue(missing);
    await expect(withExportSource(component, operation)).rejects.toThrow("window unavailable");
    expect(operation).toHaveBeenCalledOnce();
    mocks.request.mockClear();
    await expect(withExportSource(component, operation)).rejects.toBe(missing);
    expect(mocks.request).toHaveBeenCalledOnce();
  });
});
