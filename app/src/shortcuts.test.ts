import { describe, expect, it } from "vitest";
import { assign, bindingsOf, cleanKeymap, comboFromEvent, matchShortcut, PRESETS, shellConflict, unassign } from "./shortcuts";

const key = (key: string, mods: Partial<{ ctrl: boolean; shift: boolean; alt: boolean }> = {}, code = "") => ({
  key,
  code,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
  metaKey: false,
});

describe("终端级快捷键", () => {
  it("识别默认快捷键", () => {
    expect(matchShortcut(key("T", { ctrl: true, shift: true }))).toEqual({ type: "newTab" });
    expect(matchShortcut(key("Tab", { ctrl: true }))).toEqual({ type: "nextTab" });
    expect(matchShortcut(key("Tab", { ctrl: true, shift: true }))).toEqual({ type: "prevTab" });
    expect(matchShortcut(key("3", { alt: true }, "Digit3"))).toEqual({ type: "gotoTab", index: 2 });
  });

  it("缩放、设置、命令面板", () => {
    expect(matchShortcut(key("=", { ctrl: true }, "Equal"))).toEqual({ type: "zoomIn" });
    expect(matchShortcut(key("+", { ctrl: true, shift: true }, "Equal"))).toEqual({ type: "zoomIn" });
    expect(matchShortcut(key("-", { ctrl: true }, "Minus"))).toEqual({ type: "zoomOut" });
    expect(matchShortcut(key("-", { ctrl: true }, "NumpadSubtract"))).toEqual({ type: "zoomOut" });
    expect(matchShortcut(key("0", { ctrl: true }, "Digit0"))).toEqual({ type: "zoomReset" });
    expect(matchShortcut(key(",", { ctrl: true }, "Comma"))).toEqual({ type: "settings" });
    expect(matchShortcut(key("P", { ctrl: true, shift: true }, "KeyP"))).toEqual({ type: "palette" });
    expect(matchShortcut(key("R", { ctrl: true, shift: true }, "KeyR"))).toEqual({ type: "history" });
    // AltGr（= Ctrl+Alt）打出来的字符不能被当成缩放
    expect(matchShortcut(key("-", { ctrl: true, alt: true }, "Minus"))).toBeNull();
  });

  it("shell 和 vim 常用的键不拦", () => {
    for (const k of [
      key("Tab"),
      key("r", { ctrl: true }),
      key("c", { ctrl: true }),
      key(" ", { ctrl: true }),
      key("w", { ctrl: true }),
      key("b", { alt: true }),
      key("ArrowUp"),
    ]) {
      expect(matchShortcut(k)).toBeNull();
    }
  });

  it("自定义：改键、从别的操作拿走、预设", () => {
    const ctrlB = key("b", { ctrl: true }, "KeyB");
    expect(matchShortcut(ctrlB)).toBeNull();
    const vs = PRESETS.find((p) => p.id === "vscode")!.keys;
    expect(matchShortcut(ctrlB, vs)).toEqual({ type: "toggleSidebar" });
    // 默认的 Ctrl+Shift+B 被预设整组替换掉了
    expect(matchShortcut(key("B", { ctrl: true, shift: true }, "KeyB"), vs)).toBeNull();

    const { keymap, takenFrom } = assign({}, "history", "Ctrl+Shift+P");
    expect(takenFrom).toBe("palette");
    expect(bindingsOf("palette", keymap)).toEqual([]);
    expect(matchShortcut(key("P", { ctrl: true, shift: true }, "KeyP"), keymap)).toEqual({ type: "history" });
    expect(bindingsOf("history", unassign(keymap, "history", "Ctrl+Shift+R"))).toEqual(["Ctrl+Shift+P"]);
  });

  it("按键写法和提醒", () => {
    expect(comboFromEvent(key("Control", { ctrl: true }, "ControlLeft"))).toBeNull();
    expect(comboFromEvent(key("F5", {}, "F5"))).toBe("F5");
    expect(comboFromEvent(key("ArrowUp", { alt: true }, "ArrowUp"))).toBe("Alt+Up");
    expect(shellConflict("Ctrl+B")).toContain("tmux");
    expect(shellConflict("Ctrl+Shift+B")).toBeNull();
    expect(shellConflict("Alt+Q")).not.toBeNull();
    expect(cleanKeymap({ newTab: ["Ctrl+T", 3], bogus: ["x"] })).toEqual({ newTab: ["Ctrl+T"] });
  });
});
