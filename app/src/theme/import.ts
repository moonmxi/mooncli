// 配色导入（docs/03 §1）：把 Windows Terminal 的配色方案（settings.json 里 schemes 的一项，
// 或 windowsterminalthemes.dev 这类网站导出的 JSON）变成 mooncli 的主题。
// 终端的 16 色原样使用；界面的颜色（面板、边框、强调色）从背景、前景和蓝色推出来。
import type { Theme } from "./theme";

const HEX = /^#[0-9a-f]{6}$/i;

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1, 7), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function hex([r, g, b]: [number, number, number]): string {
  return `#${[r, g, b].map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0")).join("").toUpperCase()}`;
}

/** 把 a 往 b 混 t（0–1） */
export function mix(a: string, b: string, t: number): string {
  const x = rgb(a);
  const y = rgb(b);
  return hex([x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t]);
}

/** WCAG 相对亮度 */
export function luminance(c: string): number {
  const ch = rgb(c).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

/** Windows Terminal 的颜色名 → mooncli 的（purple 叫 magenta，cursorColor 叫 cursor） */
const WT_KEYS: Record<string, string> = {
  background: "background",
  foreground: "foreground",
  cursorColor: "cursor",
  selectionBackground: "selectionBackground",
  black: "black",
  red: "red",
  green: "green",
  yellow: "yellow",
  blue: "blue",
  purple: "magenta",
  magenta: "magenta",
  cyan: "cyan",
  white: "white",
  brightBlack: "brightBlack",
  brightRed: "brightRed",
  brightGreen: "brightGreen",
  brightYellow: "brightYellow",
  brightBlue: "brightBlue",
  brightPurple: "brightMagenta",
  brightMagenta: "brightMagenta",
  brightCyan: "brightCyan",
  brightWhite: "brightWhite",
};

const REQUIRED = ["background", "foreground", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/** 主题名：只留字母数字，前面加 custom-，免得和内置主题撞名 */
export function themeId(name: string): string {
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return `custom-${slug || "theme"}`;
}

/**
 * 解析一个配色方案。给的是整份 WT 设置（带 schemes 数组）时取全部方案。
 * 格式不对抛出带中文说明的错误。
 */
export function importWindowsTerminal(text: string): Theme[] {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    throw new Error("不是合法的 JSON");
  }
  const list = Array.isArray(data)
    ? data
    : typeof data === "object" && data !== null && Array.isArray((data as { schemes?: unknown }).schemes)
      ? (data as { schemes: unknown[] }).schemes
      : [data];
  const themes = list.map((s, i) => schemeToTheme(s, i));
  if (themes.length === 0) throw new Error("里面没有配色方案");
  return themes;
}

function schemeToTheme(s: unknown, index: number): Theme {
  if (typeof s !== "object" || s === null) throw new Error(`第 ${index + 1} 个配色方案不是对象`);
  const src = s as Record<string, unknown>;
  const t: Record<string, string> = {};
  for (const [from, to] of Object.entries(WT_KEYS)) {
    const v = src[from];
    if (typeof v === "string" && HEX.test(v.slice(0, 7))) t[to] = v.slice(0, 7).toUpperCase();
  }
  const missing = REQUIRED.filter((k) => !t[k]);
  if (missing.length) throw new Error(`配色方案缺少颜色：${missing.join("、")}（需要 Windows Terminal 的配色格式）`);
  for (const c of ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"]) {
    t[`bright${c}`] ??= t[c.toLowerCase()];
  }
  const bg = t.background;
  const fg = t.foreground;
  const kind = luminance(bg) > 0.4 ? "light" : "dark";
  t.cursor ??= fg;
  t.cursorAccent = bg;
  t.selectionBackground ??= mix(bg, t.blue, 0.25);
  const accent = t.blue;
  const name = typeof src.name === "string" && src.name.trim() ? src.name.trim() : `导入的配色 ${index + 1}`;
  return {
    name: themeId(name),
    displayName: name,
    kind,
    description: "从 Windows Terminal 配色导入",
    terminal: t,
    ui: {
      surface0: bg,
      surface1: mix(bg, fg, 0.04),
      surface2: mix(bg, fg, 0.08),
      border: mix(bg, fg, 0.14),
      text: fg,
      textMuted: mix(fg, bg, 0.38),
      accent,
      accentSoft: mix(bg, accent, kind === "light" ? 0.14 : 0.24),
      onAccent: luminance(accent) > 0.45 ? "#1A1A1A" : "#FFFFFF",
      success: t.green,
      warning: t.yellow,
      danger: t.red,
    },
  };
}
