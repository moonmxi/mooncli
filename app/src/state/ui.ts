// 浮层的状态：复制提醒、小提示、右键菜单、命令面板、历史面板、设置页。
import { create } from "zustand";

export interface MenuItem {
  label: string;
  hint?: string;
  icon?: string;
  danger?: boolean;
  disabled?: boolean;
  /** 一行颜色小圆点（标签颜色） */
  swatches?: { key: string; color: string; active: boolean; label: string; onPick(): void }[];
  onSelect?(): void;
}

export type MenuEntry = MenuItem | "separator";

export type SettingsSection = "appearance" | "font" | "terminal" | "tabs" | "shell" | "agent" | "keys" | "data";

interface UiState {
  copyToast: { id: number; x: number; y: number; lines: number; chars: number } | null;
  notice: { id: number; text: string } | null;
  menu: { x: number; y: number; items: MenuEntry[] } | null;
  /** 最近一次打开右键菜单的位置：菜单里的"复制"类操作把提醒卡片放在这里 */
  lastMenuAt: { x: number; y: number };
  paletteOpen: boolean;
  historyOpen: boolean;
  /** 设置里正在录新的快捷键：这时按键不触发任何快捷键 */
  recordingKey: boolean;
  settings: { open: boolean; section: SettingsSection };
  showCopyToast(x: number, y: number, text: string): void;
  showNotice(text: string): void;
  openMenu(x: number, y: number, items: MenuEntry[]): void;
  closeMenu(): void;
  setPalette(open: boolean): void;
  setHistory(open: boolean): void;
  setRecordingKey(on: boolean): void;
  openSettings(section?: SettingsSection): void;
  closeSettings(): void;
}

let seq = 0;

export const useUi = create<UiState>((set) => ({
  copyToast: null,
  notice: null,
  menu: null,
  lastMenuAt: { x: 0, y: 0 },
  paletteOpen: false,
  historyOpen: false,
  recordingKey: false,
  settings: { open: false, section: "appearance" },
  showCopyToast(x, y, text) {
    const lines = text.split(/\r?\n/).length;
    set({ copyToast: { id: ++seq, x, y, lines, chars: [...text].length } });
  },
  showNotice(text) {
    set({ notice: { id: ++seq, text } });
  },
  openMenu(x, y, items) {
    set({ menu: { x, y, items }, lastMenuAt: { x, y } });
  },
  closeMenu() {
    set({ menu: null });
  },
  setPalette(open) {
    set({ paletteOpen: open, menu: null, ...(open ? { historyOpen: false } : {}) });
  },
  setRecordingKey(on) {
    set({ recordingKey: on });
  },
  setHistory(open) {
    set({ historyOpen: open, menu: null, ...(open ? { paletteOpen: false } : {}) });
  },
  openSettings(section) {
    set((s) => ({ settings: { open: true, section: section ?? s.settings.section }, paletteOpen: false, historyOpen: false, menu: null }));
  },
  closeSettings() {
    set((s) => ({ settings: { ...s.settings, open: false } }));
  },
}));

/** 复制提醒里的说明文字："3 行 · 128 字" 或 "12 个字" */
export function copySummary(lines: number, chars: number): string {
  return lines > 1 ? `${lines} 行 · ${chars} 字` : `${chars} 个字`;
}

/** 让浮层不出窗口：返回调整后的左上角 */
export function clampToViewport(x: number, y: number, w: number, h: number, vw: number, vh: number, margin = 8) {
  return {
    x: Math.round(Math.min(Math.max(margin, x), vw - w - margin)),
    y: Math.round(Math.min(Math.max(margin, y), vh - h - margin)),
  };
}
