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
  remote?: { host: string; user?: string };
  shellState: ShellState;
  running?: { text?: string; kind: ProgramKind; since: number };
  lastCommand?: { text?: string; exitCode: number | null; durationMs?: number; endedAt: number };
  attention?: { reason: Attention; at: number };
  exited?: { code: number | null };
  env?: { pwsh?: string; host?: string; admin?: boolean };
  progress?: { state: number; value: number };
}

const LONG_COMMAND_MS = 30_000;
const PREFS_KEY = "mooncli.layout";

interface Prefs {
  sidebarWidth: number;
  lastOpenWidth: number;
  tabsPosition: "left" | "top";
  autoCollapse: boolean;
  theme: string;
}

function loadPrefs(): Prefs {
  const d: Prefs = {
    sidebarWidth: SIDEBAR.default,
    lastOpenWidth: SIDEBAR.default,
    tabsPosition: "left",
    autoCollapse: true,
    theme: "moon-paper",
  };
  try {
    return { ...d, ...JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") };
  } catch {
    return d;
  }
}

function savePrefs(p: Prefs) {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(p));
  } catch {
    // 存不了就算了，只是下次打开回到默认布局
  }
}

interface State extends Prefs {
  tabs: Tab[];
  activeId?: string;
  drawerOpen: boolean;
  focused: boolean;
  addTab(id: string, profile?: string): void;
  closeTab(id: string): void;
  activate(id: string): void;
  cycle(delta: number): void;
  activateIndex(i: number): void;
  rename(id: string, title?: string): void;
  apply(id: string, msg: SessionMsg, now?: number): void;
  setSidebarWidth(w: number): void;
  setTabsPosition(p: "left" | "top"): void;
  setTheme(name: string): void;
  setDrawerOpen(open: boolean): void;
  setFocused(f: boolean): void;
}

function prefsOf(s: State): Prefs {
  return {
    sidebarWidth: s.sidebarWidth,
    lastOpenWidth: s.lastOpenWidth,
    tabsPosition: s.tabsPosition,
    autoCollapse: s.autoCollapse,
    theme: s.theme,
  };
}

export const useStore = create<State>((set, get) => {
  const patch = (id: string, f: (t: Tab) => Partial<Tab>) =>
    set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, ...f(t) } : t)) }));

  return {
    ...loadPrefs(),
    tabs: [],
    drawerOpen: false,
    focused: true,

    addTab(id, profile = "日常") {
      set((s) => ({ tabs: [...s.tabs, { id, profile, title: {}, shellState: "unknown" }], activeId: id }));
    },

    closeTab(id) {
      set((s) => {
        const i = s.tabs.findIndex((t) => t.id === id);
        const tabs = s.tabs.filter((t) => t.id !== id);
        const activeId = s.activeId === id ? tabs[Math.min(i, tabs.length - 1)]?.id : s.activeId;
        return { tabs, activeId };
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
      if (tabs.length === 0) return;
      const i = tabs.findIndex((t) => t.id === activeId);
      get().activate(tabs[(i + delta + tabs.length) % tabs.length].id);
    },

    activateIndex(i) {
      const t = get().tabs[i];
      if (t) get().activate(t.id);
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

    setSidebarWidth(w) {
      set((s) => ({ sidebarWidth: w, lastOpenWidth: w > SIDEBAR.collapsed ? w : s.lastOpenWidth }));
      savePrefs(prefsOf(get()));
    },

    setTabsPosition(p) {
      set({ tabsPosition: p });
      savePrefs(prefsOf(get()));
    },

    setTheme(name) {
      set({ theme: name });
      savePrefs(prefsOf(get()));
    },

    setDrawerOpen(open) {
      set({ drawerOpen: open });
    },

    setFocused(f) {
      set({ focused: f });
    },
  };
});
