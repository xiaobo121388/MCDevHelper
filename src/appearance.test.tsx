import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AppearanceSettings } from "./AppearanceSettings";
import { applyAppearance, COLOR_PRESETS, colorPreset, DEFAULT_APPEARANCE } from "./appearance";

function luminance(hex: string) {
  const channels = [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16) / 255)
    .map((value) => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
}
function contrast(left: string, right: string) {
  const values = [luminance(left), luminance(right)].sort((a, b) => b - a);
  return (values[0] + .05) / (values[1] + .05);
}

afterEach(cleanup);

describe("appearance palettes", () => {
  it("has eight distinct presets and defaults old settings to Fluent", () => {
    expect(new Set(COLOR_PRESETS.map((preset) => preset.id)).size).toBe(8);
    expect(colorPreset().id).toBe("fluent");
    expect(colorPreset("unknown").id).toBe("fluent");
  });

  it.each(COLOR_PRESETS)("applies both color modes for $id without changing system preference", (preset) => {
    const root = document.createElement("div");
    applyAppearance({ theme: "system", color_preset: preset.id }, root);
    expect(root.dataset.theme).toBe("system");
    expect(root.dataset.colorPreset).toBe(preset.id);
    for (const mode of ["light", "dark"] as const) {
      expect(root.style.getPropertyValue(`--preset-accent-${mode}`)).toBe(preset[mode].accent);
      expect(root.style.getPropertyValue(`--preset-contrast-${mode}`)).toBe(preset[mode].contrast);
    }
    applyAppearance(DEFAULT_APPEARANCE, root);
    expect(root.style.getPropertyValue("--preset-accent-dark")).toBe(COLOR_PRESETS[0].dark.accent);
  });

  it.each(COLOR_PRESETS)("keeps normal-size accent text readable in $id", (preset) => {
    for (const mode of ["light", "dark"] as const) {
      const tone = preset[mode];
      expect(contrast(tone.accent, tone.contrast)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tone.hover, tone.contrast)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tone.accent, tone.soft)).toBeGreaterThanOrEqual(4.5);
    }
  });
});

describe("appearance controls", () => {
  it("offers color swatches independently of display mode", () => {
    const change = vi.fn();
    render(<AppearanceSettings value={{ theme: "dark", color_preset: "graphite" }} onChange={change} />);
    expect(within(screen.getByRole("radiogroup", { name: "配色预设" })).getAllByRole("radio")).toHaveLength(8);
    expect(screen.getByRole("radio", { name: "石墨灰" })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: "macOS 蓝" }));
    expect(change).toHaveBeenLastCalledWith({ theme: "dark", color_preset: "cupertino" });
    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));
    expect(change).toHaveBeenLastCalledWith({ theme: "light", color_preset: "graphite" });
  });

  it("restores the default appearance without adding unrelated settings", () => {
    const change = vi.fn();
    render(<AppearanceSettings value={{ theme: "dark", color_preset: "rose" }} onChange={change} />);
    fireEvent.click(screen.getByRole("button", { name: "恢复默认外观" }));
    expect(change).toHaveBeenCalledWith(DEFAULT_APPEARANCE);
  });

  it("disables all appearance controls while settings are being saved", () => {
    render(<AppearanceSettings value={DEFAULT_APPEARANCE} onChange={vi.fn()} disabled />);
    for (const radio of screen.getAllByRole("radio")) expect(radio).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复默认外观" })).toBeDisabled();
  });
});
