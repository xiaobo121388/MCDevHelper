import { requestExportDirectory } from "./windows";
import type { ComponentSummary } from "./types";

class ExportSourceCancelled extends Error {
  readonly code = "export_source_cancelled";
}

export const isExportSourceCancelled = (error: unknown) => !!error && typeof error === "object" && "code" in error && error.code === "export_source_cancelled";

export async function withExportSource<T>(component: ComponentSummary, operation: () => Promise<T>): Promise<T> {
  try { return await operation(); }
  catch (error) {
    if (!error || typeof error !== "object" || !("code" in error) || error.code !== "pack_location_required") throw error;
    const path = await requestExportDirectory(component, "source");
    if (!path) throw new ExportSourceCancelled();
    return operation();
  }
}
