import { describe, expect, it } from "vitest";
import {
  DEFAULTS,
  chooseTheme,
  effectiveFontSize,
  effectiveUiScale,
  mergeSettings,
  pickTheme,
  zoomStep,
} from "./settings";

describe("设置合并", () => {
  it("空的保存值得到默认设置", () => {
    expect(mergeSettings({})).toEqual(DEFAULTS);
    expect(mergeSettings(undefined)).toEqual(DEFAULTS);
  });

  it("只覆盖写了的字段，类型不对的丢掉", () => {
    const m = mergeSettings({ zoom: 1.25, font: { size: "big", lineHeight: 1.4 }, unknown: 1 });
    expect(m.zoom).toBe(1.25);
    expect(m.font.size).toBe(14);
    expect(m.font.lineHeight).toBe(1.4);
    expect(m.font.family).toBe(DEFAULTS.font.family);
  });

  it("可空字段和主题的两种写法", () => {
    expect(mergeSettings({ shell: { path: "C:/pwsh.exe" } }).shell.path).toBe("C:/pwsh.exe");
    expect(mergeSettings({ background: { image: "builtin:mist" } }).background.image).toBe("builtin:mist");
    expect(mergeSettings({ theme: "frost" }).theme).toBe("frost");
    expect(mergeSettings({ theme: { light: "celadon", dark: "night-voyage", follow: "system" } }).theme).toEqual({
      light: "celadon",
      dark: "night-voyage",
      follow: "system",
    });
  });

  it("旧版本保存的 4.5 对比度迁移成 3，新版本保存的保留", () => {
    expect(mergeSettings({ terminal: { minimumContrastRatio: 4.5 } }).terminal.minimumContrastRatio).toBe(3);
    expect(mergeSettings({ version: 2, terminal: { minimumContrastRatio: 4.5 } }).terminal.minimumContrastRatio).toBe(4.5);
    expect(mergeSettings({ terminal: { minimumContrastRatio: 5 } }).terminal.minimumContrastRatio).toBe(5);
  });

  it("超出范围的值被收回", () => {
    const m = mergeSettings({ zoom: 9, uiScale: 0.1, font: { size: 2 } });
    expect([m.zoom, m.uiScale, m.font.size]).toEqual([2, 0.8, 8]);
  });
});

describe("缩放", () => {
  it("按浏览器一样的档位走", () => {
    expect(zoomStep(1, 1)).toBe(1.1);
    expect(zoomStep(1, -1)).toBe(0.9);
    expect(zoomStep(2, 1)).toBe(2);
    expect(zoomStep(0.5, -1)).toBe(0.5);
    expect(zoomStep(1.04, 1)).toBe(1.1);
  });

  it("整体缩放乘到界面尺寸和终端字号上", () => {
    const s = mergeSettings({ zoom: 1.25, uiScale: 1.2, font: { size: 13 } });
    expect(effectiveUiScale(s)).toBe(1.5);
    expect(effectiveFontSize(s)).toBe(16.5);
  });
});

describe("主题选择", () => {
  it("跟随系统", () => {
    const c = { light: "frost", dark: "night-voyage", follow: "system" as const };
    expect(pickTheme(c, true)).toBe("night-voyage");
    expect(pickTheme(c, false)).toBe("frost");
    expect(pickTheme("celadon", true)).toBe("celadon");
  });

  it("在设置页选中主题", () => {
    expect(chooseTheme(DEFAULTS.theme, "frost", "light")).toEqual({ light: "frost", dark: "night-voyage", follow: "off" });
    expect(chooseTheme(DEFAULTS.theme, "night-voyage", "dark")).toBe("night-voyage");
    expect(chooseTheme({ light: "frost", dark: "night-voyage", follow: "system" }, "night-voyage", "dark")).toEqual({
      light: "frost",
      dark: "night-voyage",
      follow: "system",
    });
    expect(chooseTheme("night-voyage", "celadon", "light")).toEqual({ light: "celadon", dark: "night-voyage", follow: "off" });
  });
});
