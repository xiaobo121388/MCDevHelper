import { describe, expect, it } from "vitest";
import { mergeComponent } from "./workspace";
import type { ComponentSummary, DiscoveryResult } from "./types";

const component: ComponentSummary = { id: "new", name: "New", kind: "addon", path: "D:/New", origin: { kind: "single", source_id: "source" }, manifests: [], tags: [], favorite: false, size_bytes: 1 };
describe("incremental workspace updates", () => {
  it("inserts once, registers a single source and keeps unrelated data", () => {
    const initial: DiscoveryResult = { components: [], sources: [], warnings: [{ path: "D:/Broken", message: "invalid" }] };
    const added = mergeComponent(initial, component);
    expect(added.sources).toEqual([{ id: "source", kind: "single", path: "D:/New" }]);
    expect(added.warnings).toBe(initial.warnings);
    const updated = mergeComponent(added, { ...component, name: "Renamed" });
    expect(updated.components).toHaveLength(1);
    expect(updated.components[0].name).toBe("Renamed");
    expect(updated.sources).toHaveLength(1);
    expect(initial.components).toEqual([]);
  });
});
