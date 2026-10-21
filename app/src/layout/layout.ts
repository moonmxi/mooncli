// 侧栏缩放和响应式布局的规则，见 docs/06-smart-tabs.md §1.1、§1.2。
// 这里只有纯函数，方便测试；界面组件只管把结果画出来。

export const SIDEBAR = {
  collapsed: 52,
  min: 160,
  max: 360,
  default: 220,
  /** 拖到这个宽度以下松手就收起 */
  snapBelow: 120,
  /** 从收起状态往外拖超过这个宽度才展开 */
  expandAfter: 100,
  keyboardStep: 16,
} as const;

export const PANEL = { min: 240, max: 480, default: 320 } as const;

/** 14px Cascadia Code 的字符宽度，docs 里的像素断点按它给出 */
export const REFERENCE_CHAR_WIDTH = 8.43;
const TERMINAL_PADDING = 32;

export type Density = "collapsed" | "compact" | "standard" | "wide";
export type Breakpoint = "xl" | "lg" | "md" | "sm";

export function density(width: number): Density {
  if (width <= SIDEBAR.collapsed) return "collapsed";
  if (width < 200) return "compact";
  if (width < 280) return "standard";
  return "wide";
}

/** 拖动过程中的宽度（还没吸附）。 */
export function dragWidth(startWidth: number, dx: number): number {
  const w = startWidth + dx;
  if (startWidth <= SIDEBAR.collapsed) {
    return w > SIDEBAR.expandAfter ? Math.min(SIDEBAR.max, Math.max(SIDEBAR.min, w)) : SIDEBAR.collapsed;
  }
  return Math.min(SIDEBAR.max, Math.max(SIDEBAR.collapsed, w));
}

/** 拖动中显示的档位：低于吸附线时预告"松手收起"。 */
export function dragPreview(width: number): Density {
  return width < SIDEBAR.snapBelow ? "collapsed" : density(Math.max(width, SIDEBAR.min));
}

/** 松手时吸附。 */
export function releaseWidth(width: number): number {
  const w = Math.round(width);
  if (w < SIDEBAR.snapBelow) return SIDEBAR.collapsed;
  if (w < SIDEBAR.min) return SIDEBAR.min;
  return Math.min(SIDEBAR.max, w);
}

/** 键盘调整：←/→ 每次 16px；从收起按 → 直接到最小展开宽度。 */
export function keyboardWidth(width: number, key: "ArrowLeft" | "ArrowRight"): number {
  if (width <= SIDEBAR.collapsed) return key === "ArrowRight" ? SIDEBAR.min : SIDEBAR.collapsed;
  const w = width + (key === "ArrowRight" ? SIDEBAR.keyboardStep : -SIDEBAR.keyboardStep);
  if (w < SIDEBAR.min) return SIDEBAR.collapsed;
  return Math.min(SIDEBAR.max, w);
}

export function toggleCollapsed(width: number, lastOpen: number): number {
  return width <= SIDEBAR.collapsed ? Math.max(SIDEBAR.min, lastOpen) : SIDEBAR.collapsed;
}

export function breakpoint(windowWidth: number, charWidth = REFERENCE_CHAR_WIDTH): Breakpoint {
  // 字号越大，越早触发收起
  const s = charWidth / REFERENCE_CHAR_WIDTH;
  if (windowWidth >= 1280 * s) return "xl";
  if (windowWidth >= 960 * s) return "lg";
  if (windowWidth >= 720 * s) return "md";
  return "sm";
}

export interface LayoutInput {
  windowWidth: number;
  charWidth?: number;
  tabsPosition: "left" | "top";
  /** 用户拖出来的侧栏宽度（52 表示用户主动收起） */
  sidebarWidth: number;
  autoCollapse: boolean;
  drawerOpen: boolean;
  panelOpen: boolean;
  panelPinned: boolean;
  panelWidth: number;
}

export interface Layout {
  bp: Breakpoint;
  sidebar: { mode: "docked" | "drawer" | "none"; width: number; density: Density; drawerOpen: boolean };
  panel: { mode: "docked" | "overlay" | "closed"; width: number };
  terminalWidth: number;
}

export function computeLayout(i: LayoutInput): Layout {
  const cw = i.charWidth ?? REFERENCE_CHAR_WIDTH;
  const bp = breakpoint(i.windowWidth, cw);
  const min60 = 60 * cw + TERMINAL_PADDING;

  let sidebarMode: Layout["sidebar"]["mode"] = i.tabsPosition === "top" ? "none" : "docked";
  let sidebarWidth = i.sidebarWidth;
  if (sidebarMode === "docked") {
    if (bp === "sm") sidebarMode = "drawer";
    else if (bp === "md" && i.autoCollapse) sidebarWidth = SIDEBAR.collapsed;
  }

  let panelMode: Layout["panel"]["mode"] = "closed";
  if (i.panelOpen) {
    if (bp === "xl") panelMode = "docked";
    else if (bp === "lg" && i.panelPinned) panelMode = "docked";
    else panelMode = "overlay";
  }

  const docked = () =>
    (sidebarMode === "docked" ? sidebarWidth : 0) + (panelMode === "docked" ? i.panelWidth : 0);
  // 终端少于 60 列时，先让右侧面板浮起来，再让侧栏变成抽屉
  if (i.windowWidth - docked() < min60 && panelMode === "docked") panelMode = "overlay";
  if (i.windowWidth - docked() < min60 && sidebarMode === "docked") sidebarMode = "drawer";

  const drawerWidth = Math.max(sidebarWidth, SIDEBAR.default);
  return {
    bp,
    sidebar: {
      mode: sidebarMode,
      width: sidebarMode === "drawer" ? drawerWidth : sidebarWidth,
      density: density(sidebarMode === "drawer" ? drawerWidth : sidebarWidth),
      drawerOpen: sidebarMode === "drawer" && i.drawerOpen,
    },
    panel: { mode: panelMode, width: i.panelWidth },
    terminalWidth: i.windowWidth - docked(),
  };
}
