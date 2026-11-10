import { invoke } from "@tauri-apps/api/core";

export type ConfigData = Record<string, unknown>;
export type ConfigEdit = { path: string[]; value: unknown };
export type ConfigSnapshot = { path: string; revision: string | null; data: ConfigData };
export type Skin = { name: string; path: string; texture: string; slim: boolean; kind: "vanilla" | "netease4d" };
export type SkinCatalog = { skins: Skin[]; warnings: string[] };
export type CustomSkin = { path: string; slim: boolean };
export type GameInstallation = { path: string; version: string | null; source: "mcstudio" | "sibling" | "configured" };
export type GameCatalog = { installations: GameInstallation[]; warnings: string[]; selected_path?: string | null };

export const editorApi = {
  games: (componentId: string, gameExecutablePath: string) => invoke<GameCatalog>("get_mcdk_game_installations", { componentId, gameExecutablePath }),
  load: (componentId: string) => invoke<ConfigSnapshot>("get_mcdk_config", { componentId }),
  save: (componentId: string, revision: string | null, edits: ConfigEdit[]) => invoke<ConfigSnapshot>("save_mcdk_config", { componentId, revision, edits }),
  catalog: (gameExecutablePath: string) => invoke<SkinCatalog>("get_mcdk_skin_catalog", { gameExecutablePath }),
  preview: (componentId: string, path: string) => invoke<string>("get_mcdk_skin_preview", { componentId, path }),
  library: (componentId: string) => invoke<CustomSkin[]>("get_mcdk_skin_library", { componentId }),
  updateLibrary: (componentId: string, path: string, slim: boolean, remove = false) => invoke<CustomSkin[]>("update_mcdk_skin_library", { componentId, path, slim, remove }),
};

export function valueAt(data: ConfigData, key: string): unknown {
  return key.split(".").reduce<unknown>((value, part) => value && typeof value === "object" ? (value as ConfigData)[part] : undefined, data);
}

export function applyEdits(data: ConfigData, edits: ConfigEdit[]): ConfigData {
  const next = structuredClone(data);
  for (const edit of edits) {
    let target = next;
    for (const key of edit.path.slice(0, -1)) {
      const previous = target[key];
      target[key] = previous && typeof previous === "object" && !Array.isArray(previous) ? { ...previous } : {};
      target = target[key] as ConfigData;
    }
    target[edit.path[edit.path.length - 1]] = edit.value;
  }
  return next;
}

export function normalizeSkinPath(path: string) { return path.trim().replaceAll("\\", "/").replace(/\/+$/g, "").toLowerCase(); }

export function legacyVanillaMatch(path: string, game: string, skins: Skin[]): Skin | undefined {
  const gameMatch = normalizeSkinPath(game).match(/^(.*)\/\d+(?:\.\d+){2,}\/minecraft\.windows\.exe$/);
  if (!gameMatch) return;
  const previous = normalizeSkinPath(path);
  const prefix = gameMatch[1] + "/";
  if (!previous.startsWith(prefix)) return;
  const match = previous.slice(prefix.length).match(/^\d+(?:\.\d+){2,}\/data\/skin_packs\/vanilla\/(.+)$/);
  return match ? skins.find(s => s.kind === "vanilla" && normalizeSkinPath(s.texture) === match[1]) : undefined;
}
