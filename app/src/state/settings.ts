// 设置：结构和默认值与 crates/mooncli-config/src/settings.rs 对应。
// 界面持有完整设置；Rust 端保存到 settings.json，并只读取自己用得到的字段（shell、history）。
import { create } from "zustand";
import { cleanKeymap, type Keymap } from "../shortcuts";

// 终端字体回退链，按顺序找字：英文等宽 → Nerd Font 图标 → 中文 → 彩色 emoji → 符号 → 系统等宽字体。
// Noto Color Emoji 连 ASCII 的数字和 * # 都带，所以放在系统等宽字体之后，免得普通字符被它抢走。
export const DEFAULT_FONT_FAMILY =
  '"Cascadia Code", "Cascadia Mono", Consolas, "mooncli Symbols", "Microsoft YaHei UI", "Segoe UI Emoji", "Segoe UI Symbol", monospace, "Noto Color Emoji"';

export type ThemeChoice = string | { light: string; dark: string; follow: "system" | "off" };

export interface AppSettings {
  /** 设置格式的版本，用来迁移旧的默认值 */
  version: number;
  theme: ThemeChoice;
  /** 整体缩放：界面和终端一起（Ctrl+= / Ctrl+- / Ctrl+0） */
  zoom: number;
  /** 只缩放标题栏、侧栏、状态栏、面板 */
  uiScale: number;
  font: { family: string; size: number; lineHeight: number; weight: string; weightBold: string; ligatures: boolean };
  background: {
    /** null：不用背景图；"builtin:xxx"：内置；其它：用户导入的文件名 */
    image: string | null;
    fit: "cover" | "contain" | "tile" | "center";
    position: string;
    opacity: number;
    blur: number;
    brightness: number;
    saturation: number;
    overlay: number;
    scope: "terminal" | "window";
  };
  tabs: { position: "left" | "top"; sidebar: { width: number; autoCollapse: boolean }; titleSource: string };
  terminal: {
    scrollback: number;
    minimumContrastRatio: number;
    cursorStyle: "bar" | "block" | "underline";
    cursorBlink: boolean;
    smoothScrollMs: number;
    copyOnSelect: boolean;
    trimTrailingWhitespace: boolean;
    confirmMultilinePaste: boolean;
    rightClick: "copyPaste" | "menu";
    copyToast: boolean;
  };
  shell: { path: string | null; args: string[]; integration: boolean; useBundledConpty: boolean; adaptColors: boolean };
  /** keepDays：只保留最近多少天的历史，0 表示一直保留 */
  history: { enabled: boolean; ignorePatterns: string[]; importPsreadline: boolean; keepDays: number };
  /** 除了内置的 ssh、mosh、wsl……之外，也当作"连接服务器"的命令（写法和 ssh 一样，比如自己的别名） */
  remote: { connectCommands: string[] };
  /** Agent 用量小窗（docs/07，Claude Code 与 Codex）：默认关闭；widget 部分只界面用，开关部分 Rust 也读 */
  agent: {
    enabled: boolean;
    readSessionFiles: boolean;
    /** 接收官方扩展点的数据：Claude Code 的 statusLine / hooks、Codex 的 notify */
    receiveExtensions: boolean;
    /** 通过本机 Codex 的 app-server 实时查询额度 */
    liveQuota: boolean;
    /** codex 可执行文件的位置；null 时自动在 PATH 里找 */
    codexPath: string | null;
    widget: {
      /** 小窗放哪：浮动卡片 / 侧栏底部 / 不显示卡片（只留状态栏一段） */
      style: "floating" | "sidebar" | "off";
      /** 状态栏里显示一段额度摘要 */
      statusBar: boolean;
      anchor: "bottom-right" | "bottom-left" | "top-right" | "top-left";
      opacity: number;
      /** 进度条变警告色 / 危险色的阈值（0–1） */
      warnAt: number;
      dangerAt: number;
    };
  };
  /** 改过的快捷键（shortcuts.ts 的 Keymap） */
  keys: Keymap;
}

export const SETTINGS_VERSION = 2;

export const DEFAULTS: AppSettings = {
  version: SETTINGS_VERSION,
  theme: { light: "moon-paper", dark: "night-voyage", follow: "off" },
  zoom: 1,
  uiScale: 1,
  font: { family: DEFAULT_FONT_FAMILY, size: 14, lineHeight: 1.2, weight: "normal", weightBold: "bold", ligatures: true },
  background: {
    image: null,
    fit: "cover",
    position: "bottom right",
    opacity: 0.35,
    blur: 0,
    brightness: 1,
    saturation: 1,
    overlay: 0.6,
    scope: "terminal",
  },
  tabs: { position: "left", sidebar: { width: 220, autoCollapse: true }, titleSource: "auto" },
  terminal: {
    scrollback: 10_000,
    // 3 而不是 4.5：太高会把灰色的联想提示也加深到正文那么深，分不清哪些是输入的（docs/08 §2）
    minimumContrastRatio: 3,
    cursorStyle: "bar",
    cursorBlink: true,
    smoothScrollMs: 120,
    copyOnSelect: false,
    trimTrailingWhitespace: true,
    confirmMultilinePaste: true,
    rightClick: "copyPaste",
    copyToast: true,
  },
  shell: { path: null, args: [], integration: true, useBundledConpty: true, adaptColors: true },
  history: { enabled: true, ignorePatterns: [], importPsreadline: true, keepDays: 0 },
  remote: { connectCommands: [] },
  agent: {
    enabled: false,
    readSessionFiles: true,
    receiveExtensions: true,
    liveQuota: true,
    codexPath: null,
    widget: { style: "floating", statusBar: true, anchor: "bottom-right", opacity: 0.95, warnAt: 0.7, dangerAt: 0.9 },
  },
  keys: {},
};

export const LIMITS = {
  zoom: { min: 0.5, max: 2 },
  uiScale: { min: 0.8, max: 1.6 },
  fontSize: { min: 8, max: 36 },
  lineHeight: { min: 1, max: 1.8 },
} as const;

/** 整体缩放的档位，和浏览器一致 */
export const ZOOM_STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

export function zoomStep(current: number, dir: 1 | -1): number {
  if (dir > 0) return ZOOM_STEPS.find((z) => z > current + 0.001) ?? ZOOM_STEPS[ZOOM_STEPS.length - 1];
  return [...ZOOM_STEPS].reverse().find((z) => z < current - 0.001) ?? ZOOM_STEPS[0];
}

export const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

type Plain = Record<string, unknown>;
const isPlain = (v: unknown): v is Plain => typeof v === "object" && v !== null && !Array.isArray(v);

/** 把保存的设置叠在默认值上：缺的字段用默认值，类型不对的字段丢掉。 */
export function mergeSettings(saved: unknown, defaults: AppSettings = DEFAULTS): AppSettings {
  const walk = (d: unknown, s: unknown): unknown => {
    if (s === undefined) return d;
    if (isPlain(d)) {
      // theme 既可以是字符串也可以是对象
      if (!isPlain(s)) return typeof s === "string" && d === defaults.theme ? s : d;
      const out: Plain = {};
      for (const k of Object.keys(d)) out[k] = walk(d[k], s[k]);
      return out;
    }
    if (d === null) return typeof s === "string" || s === null ? s : d;
    if (Array.isArray(d)) return Array.isArray(s) ? s : d;
    return typeof s === typeof d ? s : d;
  };
  const m = walk(defaults, saved) as AppSettings;
  // 迁移：第 1 版的"最低对比度"默认是 4.5，没改过的换成新的默认值 3
  const savedVersion = isPlain(saved) && typeof saved.version === "number" ? saved.version : 1;
  if (savedVersion < 2 && m.terminal.minimumContrastRatio === 4.5) m.terminal.minimumContrastRatio = defaults.terminal.minimumContrastRatio;
  // 快捷键是"操作 → 按键列表"的表，默认值是空表，上面的通用合并会把它丢掉，单独处理
  m.keys = cleanKeymap(isPlain(saved) ? saved.keys : undefined);
  m.remote.connectCommands = m.remote.connectCommands.filter((c) => typeof c === "string" && /^[\w.-]{1,40}$/.test(c));
  m.version = SETTINGS_VERSION;
  m.zoom = clamp(m.zoom, LIMITS.zoom.min, LIMITS.zoom.max);
  m.uiScale = clamp(m.uiScale, LIMITS.uiScale.min, LIMITS.uiScale.max);
  m.font.size = clamp(m.font.size, LIMITS.fontSize.min, LIMITS.fontSize.max);
  return m;
}

/** 按系统深浅色决定用哪个主题。 */
export function pickTheme(choice: ThemeChoice, systemDark: boolean): string {
  if (typeof choice === "string") return choice;
  return choice.follow === "system" && systemDark ? choice.dark : choice.light;
}

/** 设置页里"选中某个主题"：保留夜间主题和跟随设置，只换日间主题；选中深色主题时直接固定。 */
export function chooseTheme(choice: ThemeChoice, name: string, kind: "light" | "dark"): ThemeChoice {
  const pair = typeof choice === "string" ? { ...(DEFAULTS.theme as Exclude<ThemeChoice, string>) } : { ...choice };
  if (kind === "dark" && pair.follow === "system") return { ...pair, dark: name };
  if (kind === "dark") return name;
  return { ...pair, light: name };
}

/** 最终生效的值：整体缩放乘到界面尺寸和终端字号上 */
export const effectiveUiScale = (s: AppSettings) => +(s.zoom * s.uiScale).toFixed(3);
export const effectiveFontSize = (s: AppSettings) => Math.round(s.font.size * s.zoom * 2) / 2;

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;
  setAll(s: AppSettings): void;
  update(f: (draft: AppSettings) => void): void;
}

let saver: ((s: AppSettings) => void) | null = null;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** 由 App 在启动时接上后端的保存函数。改动后 300ms 内没有新改动才写盘。 */
export function setSettingsSaver(fn: (s: AppSettings) => void) {
  saver = fn;
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: DEFAULTS,
  loaded: false,
  setAll(s) {
    set({ settings: s, loaded: true });
  },
  update(f) {
    const next = structuredClone(get().settings);
    f(next);
    set({ settings: mergeSettings(next) });
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => saver?.(get().settings), 300);
  },
}));
