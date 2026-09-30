import type { ComponentSummary, DiscoveryResult } from "./types";

export function mergeComponent(result: DiscoveryResult, component: ComponentSummary): DiscoveryResult {
  const exists = result.components.some((item) => item.id === component.id);
  const sources = [...result.sources];
  if (component.origin.kind === "single" && component.origin.source_id && !sources.some((source) => source.id === component.origin.source_id)) {
    sources.push({ id: component.origin.source_id, kind: "single", path: component.path });
  }
  return { ...result, sources, components: exists ? result.components.map((item) => item.id === component.id ? component : item) : [...result.components, component] };
}
