import { Check, Monitor, Moon, RotateCcw, Sun } from "lucide-react";
import type { CSSProperties } from "react";
import { COLOR_PRESETS, DEFAULT_APPEARANCE, colorPreset, type Appearance } from "./appearance";

export function AppearanceSettings({ value, onChange, disabled = false }: { value: Appearance; onChange: (appearance: Appearance) => void; disabled?: boolean }) {
  const selected = colorPreset(value.color_preset).id;
  return <fieldset className="appearance-settings" disabled={disabled}>
    <section className="appearance-section">
      <div className="section-heading"><h3>显示模式</h3><button type="button" className="appearance-reset" title="恢复默认外观" aria-label="恢复默认外观" onClick={() => onChange(DEFAULT_APPEARANCE)}><RotateCcw size={16} /></button></div>
      <div className="theme-picker" role="radiogroup" aria-label="显示模式">{([{ value: "system", label: "跟随系统", icon: Monitor }, { value: "light", label: "浅色", icon: Sun }, { value: "dark", label: "深色", icon: Moon }] as const).map(({ value: mode, label, icon: Icon }) => <label key={mode} className={value.theme === mode ? "selected" : ""}><input type="radio" name="theme" value={mode} checked={value.theme === mode} onChange={() => onChange({ ...value, color_preset: selected, theme: mode })} /><Icon size={18} /><span>{label}</span></label>)}</div>
    </section>
    <section className="appearance-section">
      <div className="section-heading"><h3>配色预设</h3><span className="appearance-current">{colorPreset(selected).name}</span></div>
      <div className="color-presets" role="radiogroup" aria-label="配色预设">{COLOR_PRESETS.map((preset) => <label key={preset.id} className={"color-preset" + (selected === preset.id ? " selected" : "")} style={{
        "--swatch-light": preset.light.accent, "--swatch-dark": preset.dark.accent,
        "--swatch-soft-light": preset.light.soft, "--swatch-soft-dark": preset.dark.soft,
      } as CSSProperties}>
        <input type="radio" name="color-preset" value={preset.id} aria-label={preset.name} checked={selected === preset.id} onChange={() => onChange({ ...value, color_preset: preset.id })} />
        <span className="preset-thumbnail" aria-hidden="true">
          <span className="preset-preview-title"><i /><i /></span>
          <span className="preset-preview-sidebar"><i /><i /><i /></span>
          <span className="preset-preview-content"><i /><i /><span /><b /></span>
        </span>
        <span className="preset-caption"><i className="preset-swatch" aria-hidden="true" /><span>{preset.name}</span><Check size={14} aria-hidden="true" className="preset-check" /></span>
      </label>)}</div>
    </section>
  </fieldset>;
}
