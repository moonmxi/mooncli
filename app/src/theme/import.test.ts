import { describe, expect, it } from "vitest";
import { components } from "./theme";
import { importWindowsTerminal, luminance, mix, themeId } from "./import";

// Windows Terminal 自带的 One Half Light
const ONE_HALF_LIGHT = {
  name: "One Half Light",
  black: "#383A42",
  red: "#E45649",
  green: "#50A14F",
  yellow: "#C18301",
  blue: "#0184BC",
  purple: "#A626A4",
  cyan: "#0997B3",
  white: "#FAFAFA",
  brightBlack: "#4F525D",
  brightRed: "#DF6C75",
  brightGreen: "#98C379",
  brightYellow: "#E4C07A",
  brightBlue: "#61AFEF",
  brightPurple: "#C577DD",
  brightCyan: "#56B5C1",
  brightWhite: "#FFFFFF",
  background: "#FAFAFA",
  foreground: "#383A42",
  cursorColor: "#4F525D",
  selectionBackground: "#FFFFFF",
};

describe("导入 Windows Terminal 配色", () => {
  it("颜色名对上，深浅自动判断，界面色能推出全部组件色", () => {
    const [t] = importWindowsTerminal(JSON.stringify(ONE_HALF_LIGHT));
    expect(t.name).toBe("custom-one-half-light");
    expect(t.kind).toBe("light");
    expect(t.terminal.magenta).toBe("#A626A4");
    expect(t.terminal.brightMagenta).toBe("#C577DD");
    expect(t.terminal.cursor).toBe("#4F525D");
    expect(t.ui.accent).toBe("#0184BC");
    // 内置的组件推导表在导入的主题上也能用
    expect(Object.values(components(t)).every((c) => /^#[0-9A-F]{6,8}$/i.test(c))).toBe(true);
  });

  it("整份 WT 设置里的 schemes 全部导入；缺颜色时报错", () => {
    const list = importWindowsTerminal(JSON.stringify({ schemes: [ONE_HALF_LIGHT, { ...ONE_HALF_LIGHT, name: "Dark One", background: "#1E1E1E", foreground: "#DDDDDD" }] }));
    expect(list.map((t) => t.kind)).toEqual(["light", "dark"]);
    expect(() => importWindowsTerminal(JSON.stringify({ name: "x", background: "#000000" }))).toThrow(/缺少颜色/);
    expect(() => importWindowsTerminal("not json")).toThrow(/JSON/);
  });

  it("小工具", () => {
    expect(mix("#000000", "#FFFFFF", 0.5)).toBe("#808080");
    expect(luminance("#FFFFFF")).toBeCloseTo(1);
    expect(themeId("  Solarized (Dark) ")).toBe("custom-solarized-dark");
  });
});
