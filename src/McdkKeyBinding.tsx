import { Keyboard, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { keyLabel, virtualKey } from "./mcdkKeys";

export function McdkKeyBinding({ label, value, disabled, onChange }: { label: string; value: unknown; disabled?: boolean; onChange: (value: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState("");
  const modifier = useRef<number | undefined>(undefined);
  useEffect(() => { if (disabled) { setRecording(false); setError(""); modifier.current = undefined; } }, [disabled]);
  const finish = (code: number) => { onChange(String(code)); setRecording(false); setError(""); modifier.current = undefined; };
  const capture = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    event.stopPropagation();
    if (event.key === "Tab" || event.key === "Escape") { setRecording(false); setError(""); modifier.current = undefined; if (event.key === "Escape") event.preventDefault(); return; }
    event.preventDefault();
    if (event.repeat) return;
    const code = virtualKey(event.nativeEvent);
    const isModifier = [16, 17, 18, 91, 92].includes(code ?? 0);
    const modifierCount = [event.ctrlKey, event.shiftKey, event.altKey, event.metaKey].filter(Boolean).length;
    if (modifierCount > (isModifier ? 1 : 0)) { modifier.current = undefined; setError("MCDK 仅支持单个按键"); return; }
    if (!code) { setError("此按键无法识别"); return; }
    if (isModifier) { modifier.current = code; return; }
    finish(code);
  };
  return <div className="mcdk-key-field"><span className="mcdk-input-action">
    <button type="button" className="mcdk-key-trigger" aria-label={label + "按键"} aria-pressed={recording} disabled={disabled}
      title={label + "按键" + (value ? " · VK " + value : "")} onClick={() => { setRecording(v => !v); setError(""); modifier.current = undefined; }}
      onBlur={() => { setRecording(false); setError(""); modifier.current = undefined; }} onKeyDown={capture}
      onKeyUp={event => { if (!recording) return; event.stopPropagation(); if (modifier.current !== undefined && virtualKey(event.nativeEvent) === modifier.current) { event.preventDefault(); finish(modifier.current); } }}>
      <Keyboard size={16} /><kbd>{recording ? "等待按键…" : keyLabel(value)}</kbd>
    </button><button type="button" title={"清除" + label + "按键"} aria-label={"清除" + label + "按键"} disabled={disabled || !value} onClick={() => onChange("")}><X size={15} /></button>
  </span>{error && <span className="mcdk-key-error" role="alert">{error}</span>}</div>;
}
