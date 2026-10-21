import { describe, expect, it } from "vitest";
import { builtinThemes, components, cssVariables, xtermTheme } from "./theme";

describe("主题", () => {
  it("五套内置主题按顺序加载", () => {
    expect(builtinThemes.map((t) => t.name)).toEqual(["moon-paper", "frost", "sakura-dawn", "celadon", "night-voyage"]);
  });

  it("组件色按推导表生成，overrides 优先", () => {
    const t = builtinThemes[0];
    const c = components(t);
    expect(c["tab.indicator"]).toBe(t.ui.accent);
    expect(c["scrollbar.thumb"]).toBe(`${t.ui.text}40`);
    expect(c["agentWidget.background"]).toBe(`${t.ui.surface1}E6`);
    expect(components({ ...t, overrides: { "tab.indicator": "#FF0000" } })["tab.indicator"]).toBe("#FF0000");
  });

  it("CSS 变量和 xterm 配色", () => {
    const t = builtinThemes[1];
    const v = cssVariables(t);
    expect(v["--accent"]).toBe(t.ui.accent);
    expect(v["--c-tab-active-background"]).toBe(t.ui.surface0);
    const x = xtermTheme(t);
    expect(x.background).toBe(t.terminal.background);
    expect(x.brightWhite).toBe(t.terminal.brightWhite);
  });
});
