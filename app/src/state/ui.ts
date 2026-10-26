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

export type SettingsSection = "appearance" | "font" | "terminal" | "tabs" | "shell" | "agent" | "keys" | "data" | "about";

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
  /** 补全面板（docs/04 §4）：开在哪个终端上；pinned = 钉在右侧常驻 */
  completion: { id: string; pinned: boolean; seq: number } | null;
  openCompletion(id: string): void;
  closeCompletion(): void;
  pinCompletion(pinned: boolean): void;
  /** 哪个终端开着查找框 */
  findId: string | null;
  setFind(id: string | null): void;
  /** 用户点状态栏暂时收起了浮动的额度卡片 */
  agentHidden: boolean;
  toggleAgentHidden(): void;
  setAgentHidden(on: boolean): void;
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
  completion: null,
  openCompletion(id) {
    // 已经钉住时再按一次：在常驻面板里请求一次 pwsh 补全（seq 变了面板就会重新请求）
    set((s) => ({ completion: { id, pinned: s.completion?.pinned ?? false, seq: (s.completion?.seq ?? 0) + 1 }, menu: null }));
  },
  closeCompletion() {
    set({ completion: null });
  },
  pinCompletion(pinned) {
    set((s) => (s.completion ? { completion: { ...s.completion, pinned } } : {}));
  },
  findId: null,
  setFind(id) {
    set({ findId: id });
  },
  agentHidden: false,
  toggleAgentHidden() {
    set((s) => ({ agentHidden: !s.agentHidden }));
  },
  setAgentHidden(on) {
    set({ agentHidden: on });
  },
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
