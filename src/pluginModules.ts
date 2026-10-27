import type { PanelModule } from "../sdk/mcdh";

export async function loadPluginModule(url: string): Promise<PanelModule> {
  const module = await import(/* @vite-ignore */ url) as PanelModule;
  if (typeof module.mount !== "function") throw new Error("插件面板必须导出 mount(container, context)。");
  return module;
}
