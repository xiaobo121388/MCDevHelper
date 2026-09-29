import type { AppSettings, ColorPreset } from "./types";

type AccentColors = { accent: string; hover: string; soft: string; contrast: string };
export type ColorPresetDefinition = { id: ColorPreset; name: string; light: AccentColors; dark: AccentColors };
export type Appearance = Pick<AppSettings, "theme" | "color_preset">;
export const DEFAULT_APPEARANCE: Appearance = { theme: "system", color_preset: "fluent" };

export const COLOR_PRESETS: readonly ColorPresetDefinition[] = [
  { id: "fluent", name: "Fluent 蓝",
    light: { accent: "#0f6cbd", hover: "#115ea3", soft: "#eaf2fa", contrast: "#ffffff" },
    dark: { accent: "#70b5f9", hover: "#91c8ff", soft: "#263547", contrast: "#101820" } },
  { id: "cupertino", name: "macOS 蓝",
    light: { accent: "#0066cc", hover: "#0055ad", soft: "#eaf3ff", contrast: "#ffffff" },
    dark: { accent: "#73afff", hover: "#95c3ff", soft: "#24344d", contrast: "#101a2b" } },
  { id: "graphite", name: "石墨灰",
    light: { accent: "#54545c", hover: "#3a3a40", soft: "#ededf0", contrast: "#ffffff" },
    dark: { accent: "#c7c7cc", hover: "#e0e0e4", soft: "#373739", contrast: "#1c1c1e" } },
  { id: "violet", name: "鸢尾紫",
    light: { accent: "#7052bf", hover: "#5d40a8", soft: "#f1edf9", contrast: "#ffffff" },
    dark: { accent: "#b8a1f2", hover: "#cbb9fa", soft: "#363044", contrast: "#20172f" } },
  { id: "rose", name: "玫瑰粉",
    light: { accent: "#b23d70", hover: "#96325e", soft: "#f9edf2", contrast: "#ffffff" },
    dark: { accent: "#ed9dbb", hover: "#f5b7ce", soft: "#442e38", contrast: "#29121d" } },
  { id: "amber", name: "琥珀金",
    light: { accent: "#8a5c00", hover: "#704a00", soft: "#f8f1e3", contrast: "#ffffff" },
    dark: { accent: "#eac26d", hover: "#f3d493", soft: "#403729", contrast: "#261c0a" } },
  { id: "cyan", name: "冰川青",
    light: { accent: "#00758a", hover: "#006072", soft: "#e8f4f6", contrast: "#ffffff" },
    dark: { accent: "#72cbdc", hover: "#98deeb", soft: "#293d41", contrast: "#122529" } },
  { id: "coral", name: "珊瑚橙",
    light: { accent: "#b34c34", hover: "#963b27", soft: "#fbefe9", contrast: "#ffffff" },
    dark: { accent: "#f1a58b", hover: "#f9bea9", soft: "#44332e", contrast: "#301710" } },
];

export function colorPreset(id?: string): ColorPresetDefinition {
  return COLOR_PRESETS.find((preset) => preset.id === id) ?? COLOR_PRESETS[0];
}

export function applyAppearance(appearance: Appearance, root = document.documentElement) {
  const preset = colorPreset(appearance.color_preset);
  root.dataset.theme = appearance.theme;
  root.dataset.colorPreset = preset.id;
  // Set both modes so system changes can be handled by CSS without reloading a window.
  for (const mode of ["light", "dark"] as const) {
    for (const [token, value] of Object.entries(preset[mode])) {
      root.style.setProperty(`--preset-${token}-${mode}`, value);
    }
  }
}
