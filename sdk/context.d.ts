import type { CommandHandler, Json, PluginContext, PluginEnvironment } from "./mcdh";
export type BridgeResult = { ok: true; value: Json } | { ok: false; error: { code: string; message: string } };
export function createContext(
  bridge: (method: string, args: Json) => Promise<BridgeResult>,
  metadata: { environment: PluginEnvironment; commands: { id: string }[] },
  onLog: (level: string, text: string) => void,
): { context: PluginContext; handlers: Map<string, CommandHandler>; dispose(): void };
