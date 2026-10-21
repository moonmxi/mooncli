import { describe, expect, it } from "vitest";
import { matchShortcut } from "./shortcuts";

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
});
