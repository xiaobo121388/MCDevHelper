import { describe, expect, it } from "vitest";
import { keyLabel, virtualKey } from "./mcdkKeys";

describe("Windows virtual key presentation", () => {
  it.each([["116", "F5"], ["135", "F24"], ["65", "A"], ["48", "0"], ["97", "Num 1"], ["32", "Space"], ["17", "Ctrl"], ["37", "Left"], ["191", "/"], ["", "未设置"], [null, "未设置"], ["7", "VK 0x07"], ["256", "未知按键（256）"]])("renders %s as %s", (value, label) => {
    expect(keyLabel(value)).toBe(label);
  });
  it.each([["KeyZ", 90], ["Digit5", 53], ["Numpad5", 101], ["NumpadEnter", 13], ["F1", 112], ["F24", 135], ["ControlRight", 17], ["Semicolon", 186], ["Space", 32]])("captures %s without deprecated keyCode", (code, value) => {
    expect(virtualKey({ code: String(code), keyCode: 0 })).toBe(value);
  });
  it("does not bind IME composition or unknown keys", () => {
    expect(virtualKey({ code: "KeyA", keyCode: 229 })).toBeUndefined();
    expect(virtualKey({ code: "KeyA", keyCode: 65, isComposing: true })).toBeUndefined();
    expect(virtualKey({ code: "Unidentified", keyCode: 0 })).toBeUndefined();
    expect(virtualKey({ code: "", keyCode: 123 })).toBe(123);
    expect(virtualKey({ code: "Numpad1", keyCode: 35 })).toBe(35);
    expect(virtualKey({ code: "Numpad1", keyCode: 97 })).toBe(97);
  });
});
