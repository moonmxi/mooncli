import { describe, expect, it } from "vitest";
import {
  SIDEBAR,
  breakpoint,
  computeLayout,
  density,
  dragPreview,
  dragWidth,
  keyboardWidth,
  releaseWidth,
  toggleCollapsed,
  type LayoutInput,
} from "./layout";

const base: LayoutInput = {
  windowWidth: 1440,
  tabsPosition: "left",
  sidebarWidth: 220,
  autoCollapse: true,
  drawerOpen: false,
  panelOpen: false,
  panelPinned: false,
  panelWidth: 320,
};

describe("侧栏宽度档位", () => {
  it("按宽度分四档", () => {
    expect(density(52)).toBe("collapsed");
    expect(density(160)).toBe("compact");
    expect(density(199)).toBe("compact");
    expect(density(220)).toBe("standard");
    expect(density(280)).toBe("wide");
  });
});

describe("拖动", () => {
  it("限制在 52–360 之间", () => {
    expect(dragWidth(220, 500)).toBe(360);
    expect(dragWidth(220, -500)).toBe(52);
  });
  it("从收起状态要拖过 100px 才展开，且至少 160", () => {
    expect(dragWidth(52, 40)).toBe(52);
    expect(dragWidth(52, 60)).toBe(160);
    expect(dragWidth(52, 200)).toBe(252);
  });
  it("低于 120 时预告收起", () => {
    expect(dragPreview(110)).toBe("collapsed");
    expect(dragPreview(130)).toBe("compact");
  });
  it("松手吸附", () => {
    expect(releaseWidth(100)).toBe(SIDEBAR.collapsed);
    expect(releaseWidth(140)).toBe(160);
    expect(releaseWidth(233.6)).toBe(234);
  });
  it("键盘调整", () => {
    expect(keyboardWidth(220, "ArrowRight")).toBe(236);
    expect(keyboardWidth(160, "ArrowLeft")).toBe(52);
    expect(keyboardWidth(52, "ArrowRight")).toBe(160);
    expect(keyboardWidth(352, "ArrowRight")).toBe(360);
  });
  it("双击在收起和上次宽度之间切换", () => {
    expect(toggleCollapsed(260, 260)).toBe(52);
    expect(toggleCollapsed(52, 260)).toBe(260);
  });
});

describe("响应式", () => {
  it("断点", () => {
    expect(breakpoint(1280)).toBe("xl");
    expect(breakpoint(1279)).toBe("lg");
    expect(breakpoint(960)).toBe("lg");
    expect(breakpoint(800)).toBe("md");
    expect(breakpoint(719)).toBe("sm");
  });
  it("字号变大时更早收起", () => {
    expect(breakpoint(1000)).toBe("lg");
    expect(breakpoint(1000, 8.43 * 1.2)).toBe("md");
  });
  it("宽窗口：侧栏和面板都停靠", () => {
    const l = computeLayout({ ...base, panelOpen: true });
    expect(l.sidebar).toMatchObject({ mode: "docked", width: 220, density: "standard" });
    expect(l.panel.mode).toBe("docked");
  });
  it("中等宽度：面板浮起，钉住才停靠", () => {
    expect(computeLayout({ ...base, windowWidth: 1100, panelOpen: true }).panel.mode).toBe("overlay");
    expect(computeLayout({ ...base, windowWidth: 1100, panelOpen: true, panelPinned: true }).panel.mode).toBe("docked");
  });
  it("窄窗口自动收起侧栏，但不改用户设定", () => {
    const l = computeLayout({ ...base, windowWidth: 800, sidebarWidth: 300 });
    expect(l.sidebar).toMatchObject({ mode: "docked", width: 52, density: "collapsed" });
    const wide = computeLayout({ ...base, windowWidth: 1440, sidebarWidth: 300 });
    expect(wide.sidebar.width).toBe(300);
  });
  it("关掉自动收起时保持展开", () => {
    const l = computeLayout({ ...base, windowWidth: 800, sidebarWidth: 220, autoCollapse: false });
    expect(l.sidebar).toMatchObject({ mode: "docked", width: 220 });
  });
  it("很窄：侧栏变抽屉", () => {
    const l = computeLayout({ ...base, windowWidth: 640, drawerOpen: true });
    expect(l.sidebar).toMatchObject({ mode: "drawer", drawerOpen: true, width: 220 });
    expect(l.terminalWidth).toBe(640);
  });
  it("终端不足 60 列时侧栏让位", () => {
    // 760px 宽，关掉自动收起，侧栏 360：终端只剩 400px < 60 列
    const l = computeLayout({ ...base, windowWidth: 760, sidebarWidth: 360, autoCollapse: false });
    expect(l.sidebar.mode).toBe("drawer");
  });
  it("界面放大时侧栏占的屏幕宽度跟着放大，断点提前", () => {
    const l = computeLayout({ ...base, windowWidth: 1440, uiScale: 1.5 });
    expect(l.sidebar).toMatchObject({ width: 220, px: 330 });
    expect(l.bp).toBe("lg");
    expect(computeLayout({ ...base, windowWidth: 1300, uiScale: 1.5 }).bp).toBe("md");
  });

  it("顶部标签布局没有侧栏", () => {
    expect(computeLayout({ ...base, tabsPosition: "top" }).sidebar.mode).toBe("none");
  });
});
