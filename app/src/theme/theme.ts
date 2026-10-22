// 主题：把主题文件变成 CSS 变量和 xterm.js 的配色。规则见 docs/03-appearance.md §1。
import type { ITheme } from "@xterm/xterm";
import table from "../../../resources/theme-components.json";

export interface Theme {
  name: string;
  displayName: string;
  kind: "light" | "dark";
  description?: string;
  terminal: Record<string, string>;
  ui: Record<string, string>;
  overrides?: Record<string, string>;
}

const files = import.meta.glob<Theme>("../../../design/themes/*.json", { eager: true, import: "default" });

/** 内置主题，按 docs/08 里的顺序 */
export const BUILTIN_ORDER = ["moon-paper", "frost", "sakura-dawn", "celadon", "night-voyage"];

export const builtinThemes: Theme[] = BUILTIN_ORDER.map((name) => {
  const t = Object.values(files).find((f) => f.name === name);
  if (!t) throw new Error(`缺少内置主题 ${name}`);
  return t;
});

/** 内置主题 + 用户导入的主题（settings.customThemes） */
export function allThemes(custom: Theme[] = []): Theme[] {
  return [...builtinThemes, ...custom.filter((c) => !builtinThemes.some((b) => b.name === c.name))];
}

/** 按名字找主题，找不到（比如导入的主题被删了）用第一个内置主题 */
export function findTheme(name: string, custom: Theme[] = []): Theme {
  return allThemes(custom).find((t) => t.name === name) ?? builtinThemes[0];
}

const RULES = (table as { components: Record<string, string> }).components;

function withAlpha(hex: string, alpha: number): string {
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255);
  return hex.slice(0, 7) + a.toString(16).toUpperCase().padStart(2, "0");
}

/** 组件色：overrides 优先，否则按共用的推导表（和 Rust 端同一份）。 */
export function components(t: Theme): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, rule] of Object.entries(RULES)) {
    const override = t.overrides?.[key];
    if (override) {
      out[key] = override;
      continue;
    }
    const [base, alpha] = rule.split("/");
    const color = t.ui[base];
    out[key] = alpha ? withAlpha(color, Number(alpha)) : color;
  }
  return out;
}

/** CSS 变量名：ui.surface0 → --surface0，组件色 tab.active.background → --c-tab-active-background */
export function cssVariables(t: Theme): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [k, v] of Object.entries(t.ui)) vars[`--${k}`] = v;
  for (const [k, v] of Object.entries(components(t))) vars[`--c-${k.replace(/\./g, "-")}`] = v;
  vars["--term-bg"] = t.terminal.background;
  // 标签颜色等界面元素用到的 ANSI 色
  for (const c of ["red", "yellow", "green", "cyan", "blue", "magenta"]) vars[`--ansi-${c}`] = t.terminal[c];
  vars["--term-fg"] = t.terminal.foreground;
  return vars;
}

export function applyTheme(t: Theme, el: HTMLElement = document.documentElement) {
  for (const [k, v] of Object.entries(cssVariables(t))) el.style.setProperty(k, v);
  el.dataset.themeKind = t.kind;
  el.style.colorScheme = t.kind;
}

export function xtermTheme(t: Theme): ITheme {
  const c = t.terminal;
  return {
    background: c.background,
    foreground: c.foreground,
    cursor: c.cursor,
    cursorAccent: c.cursorAccent,
    selectionBackground: c.selectionBackground,
    selectionInactiveBackground: c.selectionBackground,
    black: c.black,
    red: c.red,
    green: c.green,
    yellow: c.yellow,
    blue: c.blue,
    magenta: c.magenta,
    cyan: c.cyan,
    white: c.white,
    brightBlack: c.brightBlack,
    brightRed: c.brightRed,
    brightGreen: c.brightGreen,
    brightYellow: c.brightYellow,
    brightBlue: c.brightBlue,
    brightMagenta: c.brightMagenta,
    brightCyan: c.brightCyan,
    brightWhite: c.brightWhite,
    scrollbarSliderBackground: components(t)["scrollbar.thumb"],
  };
}
