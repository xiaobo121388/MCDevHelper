const names: Record<number, string> = {
  8: "Backspace", 9: "Tab", 12: "Clear", 13: "Enter", 16: "Shift", 17: "Ctrl", 18: "Alt", 19: "Pause", 20: "Caps Lock",
  27: "Esc", 32: "Space", 33: "Page Up", 34: "Page Down", 35: "End", 36: "Home", 37: "Left", 38: "Up", 39: "Right", 40: "Down",
  44: "Print Screen", 45: "Insert", 46: "Delete", 91: "Left Win", 92: "Right Win", 93: "Menu",
  106: "Num *", 107: "Num +", 109: "Num -", 110: "Num .", 111: "Num /", 144: "Num Lock", 145: "Scroll Lock",
  160: "Left Shift", 161: "Right Shift", 162: "Left Ctrl", 163: "Right Ctrl", 164: "Left Alt", 165: "Right Alt",
  173: "Mute", 174: "Volume -", 175: "Volume +", 176: "Media Next", 177: "Media Previous", 178: "Media Stop", 179: "Media Play/Pause",
  186: ";", 187: "=", 188: ",", 189: "-", 190: ".", 191: "/", 192: "Backquote", 219: "[", 220: "Backslash", 221: "]", 222: "Quote", 226: "Intl Backslash",
};

export function keyLabel(value: unknown): string {
  if (value === "" || value == null) return "未设置";
  const code = Number(value);
  if (!Number.isInteger(code) || code < 1 || code > 255) return "未知按键（" + String(value) + "）";
  if (code >= 65 && code <= 90 || code >= 48 && code <= 57) return String.fromCharCode(code);
  if (code >= 112 && code <= 135) return "F" + (code - 111);
  if (code >= 96 && code <= 105) return "Num " + (code - 96);
  return names[code] ?? "VK 0x" + code.toString(16).toUpperCase().padStart(2, "0");
}

const codes: Record<string, number> = {
  Backspace: 8, Tab: 9, Enter: 13, NumpadEnter: 13, ShiftLeft: 16, ShiftRight: 16, ControlLeft: 17, ControlRight: 17,
  AltLeft: 18, AltRight: 18, Pause: 19, CapsLock: 20, Escape: 27, Space: 32, PageUp: 33, PageDown: 34, End: 35, Home: 36,
  ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, PrintScreen: 44, Insert: 45, Delete: 46,
  MetaLeft: 91, MetaRight: 92, ContextMenu: 93, NumpadMultiply: 106, NumpadAdd: 107, NumpadSubtract: 109, NumpadDecimal: 110,
  NumpadDivide: 111, NumLock: 144, ScrollLock: 145, AudioVolumeMute: 173, AudioVolumeDown: 174, AudioVolumeUp: 175,
  MediaTrackNext: 176, MediaTrackPrevious: 177, MediaStop: 178, MediaPlayPause: 179,
  Semicolon: 186, Equal: 187, Comma: 188, Minus: 189, Period: 190, Slash: 191, Backquote: 192,
  BracketLeft: 219, Backslash: 220, BracketRight: 221, Quote: 222, IntlBackslash: 226,
};
export function virtualKey(event: { code: string; keyCode: number; isComposing?: boolean }): number | undefined {
  if (event.isComposing || event.keyCode === 229) return undefined;
  // WebView2's numeric key preserves Num Lock and keyboard-layout semantics.
  if (Number.isInteger(event.keyCode) && event.keyCode > 0 && event.keyCode <= 255) return event.keyCode;
  if (/^Key[A-Z]$/.test(event.code)) return event.code.charCodeAt(3);
  if (/^Digit[0-9]$/.test(event.code)) return event.code.charCodeAt(5);
  if (/^Numpad[0-9]$/.test(event.code)) return 96 + Number(event.code.slice(6));
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.code)) return 111 + Number(event.code.slice(1));
  return codes[event.code];
}
