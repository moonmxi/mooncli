// 标签页和界面布局的状态。字段含义见 docs/06-smart-tabs.md §7。
import { create } from "zustand";
import type { ProgramKind, SessionMsg, ShellState } from "../backend/types";
import { SIDEBAR } from "../layout/layout";
import { guessKind, type TitleParts } from "./title";

export type Attention = "agent-waiting" | "agent-done" | "bell" | "notify" | "long-command";

export interface Tab {
  id: string;
  profile: string;
  title: TitleParts;
  cwd?: string;
  /** 新建时指定的起始目录（复制标签页时用） */
  initialCwd?: string;
  remote?: { host: string; user?: string };
  shellState: ShellState;
  running?: { text?: string; kind: ProgramKind; since: number };
  lastCommand?: { text?: string; exitCode: number | null; durationMs?: number; endedAt: number };
  attention?: { reason: Attention; at: number };
  exited?: { code: number | null };
  env?: { pwsh?: string; host?: string; admin?: boolean };
  progress?: { state: number; value: number };
  /** 置顶：排在最前面，快捷键关不掉（docs/06 §6） */
  pinned?: boolean;
  /** 标签颜色，取主题的 ANSI 色名 */
  color?: TabColor;
}

export const TAB_COLORS = ["red", "yellow", "green", "cyan", "blue", "magenta"] as const;
export type TabColor = (typeof TAB_COLORS)[number];

const LONG_COMMAND_MS = 30_000;
const UI_KEY = "mooncli.ui";

function loadLastOpenWidth(): number {
  try {
    return JSON.parse(localStorage.getItem(UI_KEY) ?? "{}").lastOpenWidth ?? SIDEBAR.default;
  } catch {
    return SIDEBAR.default;
  }
}

/** 显示顺序：置顶的在前，其余保持原来的顺序 */
export function orderedTabs(tabs: Tab[]): Tab[] {
  return [...tabs.filter((t) => t.pinned), ...tabs.filter((t) => !t.pinned)];
}

interface State {
  tabs: Tab[];
  activeId?: string;
  drawerOpen: boolean;
  focused: boolean;
  /** 侧栏上次展开时的宽度，双击收起后再展开要回到这里 */
  lastOpenWidth: number;
  /** 正在改名的标签页 */
  editingId?: string;
  addTab(id: string, opts?: { profile?: string; cwd?: string }): void;
  closeTab(id: string): void;
  activate(id: string): void;
  cycle(delta: number): void;
  activateIndex(i: number): void;
  rename(id: string, title?: string): void;
  setEditing(id?: string): void;
  togglePin(id: string): void;
  setColor(id: string, color?: TabColor): void;
  apply(id: string, msg: SessionMsg, now?: number): void;
  rememberOpenWidth(w: number): void;
  setDrawerOpen(open: boolean): void;
  setFocused(f: boolean): void;
}

export const useStore = create<State>((set, get) => {
  const patch = (id: string, f: (t: Tab) => Partial<Tab>) =>
    set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, ...f(t) } : t)) }));

  return {
    tabs: [],
    drawerOpen: false,
    focused: true,
    lastOpenWidth: loadLastOpenWidth(),

    addTab(id, opts = {}) {
      set((s) => ({
        tabs: [
          ...s.tabs,
          { id, profile: opts.profile ?? "日常", title: {}, shellState: "unknown", initialCwd: opts.cwd, cwd: opts.cwd },
        ],
        activeId: id,
      }));
    },

    closeTab(id) {
      set((s) => {
        const order = orderedTabs(s.tabs);
        const i = order.findIndex((t) => t.id === id);
        const rest = order.filter((t) => t.id !== id);
        const activeId = s.activeId === id ? rest[Math.min(i, rest.length - 1)]?.id : s.activeId;
        return { tabs: s.tabs.filter((t) => t.id !== id), activeId };
      });
    },

    activate(id) {
      // 切过去就算看过了，清掉提醒
      set((s) => ({
        activeId: id,
        drawerOpen: false,
        tabs: s.tabs.map((t) => (t.id === id ? { ...t, attention: undefined } : t)),
      }));
    },

    cycle(delta) {
      const { tabs, activeId } = get();
      const order = orderedTabs(tabs);
      if (order.length === 0) return;
      const i = order.findIndex((t) => t.id === activeId);
      get().activate(order[(i + delta + order.length) % order.length].id);
    },

    activateIndex(i) {
      const t = orderedTabs(get().tabs)[i];
      if (t) get().activate(t.id);
    },

    setEditing(id) {
      set({ editingId: id });
    },

    togglePin(id) {
      patch(id, (t) => ({ pinned: !t.pinned }));
    },

    setColor(id, color) {
      patch(id, () => ({ color }));
    },

    rename(id, title) {
      patch(id, (t) => ({ title: { ...t.title, user: title?.trim() || undefined } }));
    },

    apply(id, msg, now = Date.now()) {
      const background = () => get().activeId !== id || !get().focused;
      switch (msg.type) {
        case "state":
          patch(id, () => ({ shellState: msg.state }));
          break;
        case "commandStarted":
          patch(id, () => ({
            running: { text: msg.text ?? undefined, kind: guessKind(msg.text ?? undefined), since: now },
            exited: undefined,
          }));
          break;
        case "commandFinished":
          patch(id, (t) => {
            const durationMs = t.running ? now - t.running.since : undefined;
            const long = durationMs !== undefined && durationMs > LONG_COMMAND_MS;
            return {
              running: undefined,
              // 程序设置的标题只在运行期间有效
              title: { ...t.title, program: undefined, activity: undefined },
              remote: undefined,
              lastCommand: { text: msg.text ?? t.running?.text, exitCode: msg.exitCode, durationMs, endedAt: now },
              attention: long && background() ? { reason: "long-command", at: now } : t.attention,
            };
          });
          break;
        case "cwd":
          patch(id, (t) => {
            // 远程主机发来的目录（带了不同的主机名）记成远程信息，不覆盖本机目录
            const localHost = t.env?.host;
            if (msg.host && localHost && msg.host.toLowerCase() !== localHost.toLowerCase() && t.running) {
              return { remote: { host: msg.host } };
            }
            return { cwd: msg.path };
          });
          break;
        case "title":
          patch(id, (t) => ({
            title: msg.informative
              ? { ...t.title, program: msg.text, activity: msg.activity ?? undefined }
              : { ...t.title, program: undefined, activity: undefined },
          }));
          break;
        case "bell":
        case "notify":
          if (background()) patch(id, () => ({ attention: { reason: msg.type, at: now } }));
          break;
        case "progress":
          patch(id, () => ({ progress: msg.state === 0 ? undefined : { state: msg.state, value: msg.value } }));
          break;
        case "env":
          patch(id, () => ({ env: { pwsh: msg.pwsh, host: msg.host, admin: msg.admin } }));
          break;
        case "exited":
          patch(id, () => ({ exited: { code: msg.code }, running: undefined, shellState: "unknown" }));
          break;
        default:
          break;
      }
    },

    rememberOpenWidth(w) {
      if (w <= SIDEBAR.collapsed) return;
      set({ lastOpenWidth: w });
      try {
        localStorage.setItem(UI_KEY, JSON.stringify({ lastOpenWidth: w }));
      } catch {
        // 存不了就算了
      }
    },

    setDrawerOpen(open) {
      set({ drawerOpen: open });
    },

    setFocused(f) {
      set({ focused: f });
    },
  };
});
