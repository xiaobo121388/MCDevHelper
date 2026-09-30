import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type { ComponentSummary } from "./types";

class ExportSourceCancelled extends Error {
  readonly code = "export_source_cancelled";
}

export const isExportSourceCancelled = (error: unknown) => !!error && typeof error === "object" && "code" in error && error.code === "export_source_cancelled";

export async function withExportSource<T>(component: ComponentSummary, operation: () => Promise<T>): Promise<T> {
  try { return await operation(); }
  catch (error) {
    if (!error || typeof error !== "object" || !("code" in error) || error.code !== "pack_location_required") throw error;
    const path = await open({ title: "选择包体目录（BP/RP 所在目录）", directory: true, multiple: false, defaultPath: component.path });
    if (typeof path !== "string") throw new ExportSourceCancelled();
    await api.setExportSource(component.id, path);
    return operation();
  }
}
