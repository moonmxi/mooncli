// 标签页和界面布局的状态。字段含义见 docs/06-smart-tabs.md §7。
import { create } from "zustand";
import type { ProgramKind, SessionMsg, ShellState } from "../backend/types";
import { paneLeaves, removePane, SIDEBAR, splitPaneTree, type PaneNode } from "../layout/layout";
import { guessKind, kindFromChain, type TitleParts } from "./title";

/** 标签页里跑什么：平常是 pwsh 7；没装 pwsh 7 时可以临时用 Windows PowerShell 5.1，或者开一个安装 pwsh 7 的标签页 */
export type LaunchMode = "pwsh" | "windowsPowerShell" | "installPwsh";

let tabCounter = 0;
export const newTabId = () => `tab-${Date.now().toString(36)}-${++tabCounter}`;

export type Attention = "agent-waiting" | "agent-done" | "bell" | "notify" | "long-command";

export interface Tab {
  id: string;
  profile: string;
  title: TitleParts;
  cwd?: string;
  /** 新建时指定的起始目录（复制标签页时用） */
  initialCwd?: string;
  launch?: LaunchMode;
  /** 连接到的远程主机（ssh、wsl、docker exec……）。connected 之前是"正在连接"（docs/06 §4.1） */
  remote?: RemoteInfo;
  shellState: ShellState;
  /** remote：在远程主机上运行的命令 */
  running?: { text?: string; kind: ProgramKind; since: number; remote?: boolean };
  lastCommand?: { text?: string; exitCode: number | null; durationMs?: number; endedAt: number; remote?: boolean };
  /** 需要用户看一眼：后台标签页里响铃、程序发通知、长命令结束、Agent 等你确认 / 跑完一轮 */
  attention?: { reason: Attention; at: number; text?: string };
  exited?: { code: number | null };
  env?: { pwsh?: string; host?: string; admin?: boolean };
  progress?: { state: number; value: number };
  /** 当前目录所在的 git 分支（docs/06 §4） */
  git?: string;
  /** 进程树探测到的正在跑的程序链（最后一个是最里层的） */
  procs?: string[];
  /** 置顶：排在最前面，快捷键关不掉（docs/06 §6） */
  pinned?: boolean;
  /** 标签颜色，取主题的 ANSI 色名 */
  color?: TabColor;
  /** 分屏：这个会话是哪个标签页里的一个窗格（不出现在标签列表里） */
  paneOf?: string;
  /** 分屏：标签页自己（列表里那一项）记录窗格树和获得焦点的窗格 */
  split?: { tree: PaneNode; focus: string };
}

/** 一个标签页最多分几个窗格 */
export const MAX_PANES = 4;

export interface RemoteInfo {
  host: string;
  user?: string;
  via?: string;
  connected: boolean;
  /** 开始连接 / 连上的时间 */
  since: number;
  /** 远程 shell 用 OSC 7 报的目录 */
  cwd?: string;
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

/** 显示顺序：置顶的在前，其余保持原来的顺序。分屏出来的窗格不算标签页 */
export function orderedTabs(tabs: Tab[]): Tab[] {
  const list = tabs.filter((t) => !t.paneOf);
  return [...list.filter((t) => t.pinned), ...list.filter((t) => !t.pinned)];
}

/** 标签页里正在操作的那个窗格（没分屏时就是它自己） */
export function paneView(tabs: Tab[], tab: Tab | undefined): Tab | undefined {
  if (!tab?.split) return tab;
  return tabs.find((t) => t.id === tab.split!.focus) ?? tab;
}

/** 当前正在操作的终端会话：当前标签页里获得焦点的窗格 */
export function focusedTab(s: { tabs: Tab[]; activeId?: string }): Tab | undefined {
  return paneView(s.tabs, s.tabs.find((t) => t.id === s.activeId));
}

/**
 * 标签列表里显示的样子：内容（标题、目录、运行状态）跟着获得焦点的窗格走，
 * 身份（id、改的名字、颜色、置顶）是标签页自己的；任一窗格有提醒都算有提醒。
 */
export function listView(tabs: Tab[], tab: Tab): Tab {
  if (!tab.split) return tab;
  const v = paneView(tabs, tab)!;
  const attention = paneLeaves(tab.split.tree).map((id) => tabs.find((t) => t.id === id)?.attention).find(Boolean);
  return { ...v, id: tab.id, paneOf: undefined, split: tab.split, pinned: tab.pinned, color: tab.color, title: { ...v.title, user: tab.title.user }, attention };
}

/** 一个标签页的全部窗格 id（含自己），按排列顺序 */
export function panesOf(tab: Tab): string[] {
  return tab.split ? paneLeaves(tab.split.tree) : [tab.id];
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
  addTab(id: string, opts?: { profile?: string; cwd?: string; launch?: LaunchMode }): void;
  /** 关闭标签页（连同它的全部窗格） */
  closeTab(id: string): void;
  /** 把当前窗格分成两个；返回新窗格 id，已到上限时返回 null */
  splitPane(dir: "row" | "column", newId: string): string | null;
  /** 关闭一个窗格；是标签页的最后一个窗格时等于关闭标签页 */
  closePane(id: string): void;
  focusPane(id: string): void;
  /** 标记一个会话需要用户看一眼（它在后台时才记；用户正看着就不打扰） */
  flag(id: string, reason: Attention, text?: string): void;
  focusNextPane(): void;
  activate(id: string): void;
  cycle(delta: number): void;
  activateIndex(i: number): void;
  rename(id: string, title?: string): void;
  setEditing(id?: string): void;
  togglePin(id: string): void;
  /** 拖动排序：把 id 放到 target 的位置（从上往下拖放在它后面，从下往上拖放在它前面）。置顶的和没置顶的各排各的 */
  moveTab(id: string, target: string): void;
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
          {
            id,
            profile: opts.profile ?? "日常",
            title: {},
            shellState: "unknown",
            initialCwd: opts.cwd,
            cwd: opts.cwd,
            launch: opts.launch,
          },
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
        return { tabs: s.tabs.filter((t) => t.id !== id && t.paneOf !== id), activeId };
      });
    },

    splitPane(dir, newId) {
      const s = get();
      const owner = s.tabs.find((t) => t.id === s.activeId);
      if (!owner) return null;
      const panes = panesOf(owner);
      if (panes.length >= MAX_PANES) return null;
      const from = paneView(s.tabs, owner)!;
      const tree = splitPaneTree(owner.split?.tree ?? { pane: owner.id }, from.id, dir, newId);
      const pane: Tab = {
        id: newId,
        profile: owner.profile,
        title: {},
        shellState: "unknown",
        initialCwd: from.cwd,
        cwd: from.cwd,
        paneOf: owner.id,
      };
      set({
        tabs: [
          ...s.tabs.map((t) => (t.id === owner.id ? { ...t, split: { tree, focus: newId } } : t)),
          pane,
        ],
      });
      return newId;
    },

    closePane(id) {
      const s = get();
      const pane = s.tabs.find((t) => t.id === id);
      if (!pane) return;
      const owner = pane.paneOf ? s.tabs.find((t) => t.id === pane.paneOf) : pane;
      if (!owner) return;
      const all = panesOf(owner);
      const panes = all.filter((p) => p !== id);
      if (panes.length === 0) return get().closeTab(owner.id);
      const tree = owner.split ? removePane(owner.split.tree, id) : null;
      // 焦点落到被关窗格的前一个
      const nextFocus = (f: string) => (f === id ? panes[Math.max(0, all.indexOf(id) - 1)] : f);
      if (pane.id !== owner.id) {
        const split = tree && panes.length > 1 ? { tree, focus: nextFocus(owner.split!.focus) } : undefined;
        set({ tabs: s.tabs.filter((t) => t.id !== id).map((t) => (t.id === owner.id ? { ...t, split } : t)) });
        return;
      }
      // 关的是标签页自己那个窗格：下一个窗格接替它在列表里的位置、名字、颜色、置顶
      const heir = panes[0];
      const split = tree && panes.length > 1 ? { tree, focus: nextFocus(owner.split!.focus) } : undefined;
      set({
        activeId: s.activeId === owner.id ? heir : s.activeId,
        tabs: s.tabs
          .filter((t) => t.id !== heir)
          .map((t) =>
            t.id === owner.id
              ? {
                  ...s.tabs.find((x) => x.id === heir)!,
                  paneOf: undefined,
                  split,
                  pinned: owner.pinned,
                  color: owner.color,
                  title: { ...s.tabs.find((x) => x.id === heir)!.title, user: owner.title.user },
                }
              : t.paneOf === owner.id
                ? { ...t, paneOf: heir }
                : t,
          ),
      });
    },

    focusPane(id) {
      const s = get();
      const pane = s.tabs.find((t) => t.id === id);
      const ownerId = pane?.paneOf ?? id;
      set({
        tabs: s.tabs.map((t) => {
          let n = t;
          if (t.id === ownerId && t.split && t.split.focus !== id) n = { ...n, split: { ...t.split, focus: id } };
          if (t.id === id && t.attention) n = { ...n, attention: undefined };
          return n;
        }),
      });
    },

    flag(id, reason, text) {
      const s = get();
      const owner = s.tabs.find((t) => t.id === id)?.paneOf ?? id;
      if (!s.tabs.some((t) => t.id === id)) return;
      if (s.activeId === owner && s.focused && focusedTab(s)?.id === id) return;
      patch(id, () => ({ attention: { reason, at: Date.now(), text } }));
    },

    focusNextPane() {
      const s = get();
      const owner = s.tabs.find((t) => t.id === s.activeId);
      if (!owner?.split) return;
      const panes = paneLeaves(owner.split.tree);
      const { focus } = owner.split;
      get().focusPane(panes[(panes.indexOf(focus) + 1) % panes.length]);
    },

    activate(id) {
      // 给的是分屏里的某个窗格：切到它所在的标签页，并把焦点给这个窗格
      const pane = get().tabs.find((t) => t.id === id);
      if (pane?.paneOf) {
        set({ activeId: pane.paneOf, drawerOpen: false });
        get().focusPane(id);
        return;
      }
      // 切过去就算看过了，清掉提醒（只清正在看的那个窗格）
      set((s) => {
        const owner = s.tabs.find((t) => t.id === id);
        const seen = owner?.split?.focus ?? id;
        return {
          activeId: id,
          drawerOpen: false,
          tabs: s.tabs.map((t) => (t.id === seen ? { ...t, attention: undefined } : t)),
        };
      });
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

    moveTab(id, target) {
      set((s) => {
        const from = s.tabs.findIndex((t) => t.id === id);
        const to = s.tabs.findIndex((t) => t.id === target);
        if (from < 0 || to < 0 || from === to) return {};
        // 不能拖过置顶分界线
        if (!!s.tabs[from].pinned !== !!s.tabs[to].pinned) return {};
        const tabs = [...s.tabs];
        const [moving] = tabs.splice(from, 1);
        tabs.splice(tabs.findIndex((t) => t.id === target) + (from < to ? 1 : 0), 0, moving);
        return { tabs };
      });
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
      const background = () => {
        const s = get();
        const owner = s.tabs.find((t) => t.id === id)?.paneOf ?? id;
        return s.activeId !== owner || !s.focused || focusedTab(s)?.id !== id;
      };
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
              procs: undefined,
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
            const foreign = msg.host && localHost && msg.host.toLowerCase() !== localHost.toLowerCase();
            // 连着远程时带主机名的目录都是远程的（远程主机可能和本机同名）
            if (msg.host && t.remote) return { remote: { ...t.remote, cwd: msg.path } };
            if (foreign && t.running) return { remote: { host: msg.host!, connected: true, since: now, cwd: msg.path } };
            return { cwd: msg.path };
          });
          break;
        case "remoteConnecting":
          patch(id, (t) => ({
            remote: { host: msg.target.host, user: msg.target.user ?? undefined, via: msg.target.via, connected: false, since: now },
            running: t.running ? { ...t.running, kind: "ssh" } : t.running,
          }));
          break;
        case "remoteConnected":
          // 连上了：连接命令到这里就算成功完成，不再一直显示"运行中"
          patch(id, (t) => ({
            running: undefined,
            lastCommand: {
              text: t.running?.text,
              exitCode: 0,
              durationMs: t.running ? now - t.running.since : undefined,
              endedAt: now,
            },
            remote: {
              host: msg.target.host,
              user: msg.target.user ?? undefined,
              via: msg.target.via,
              connected: true,
              since: now,
              cwd: t.remote?.cwd,
            },
          }));
          break;
        case "remoteCommandStarted":
          patch(id, () => ({
            running: { text: msg.text || undefined, kind: guessKind(msg.text), since: now, remote: true },
          }));
          break;
        case "remoteCommandFinished":
          patch(id, (t) => {
            const durationMs = t.running ? now - t.running.since : undefined;
            const long = durationMs !== undefined && durationMs > LONG_COMMAND_MS;
            // 直接按回车（没读到文字）的不算一条命令
            if (!t.running?.text) return { running: undefined };
            return {
              running: undefined,
              title: { ...t.title, program: undefined, activity: undefined },
              lastCommand: { text: t.running.text, exitCode: msg.exitCode, durationMs, endedAt: now, remote: true },
              attention: long && background() ? { reason: "long-command", at: now } : t.attention,
            };
          });
          break;
        case "remoteDisconnected":
          patch(id, (t) => ({
            running: undefined,
            remote: undefined,
            title: { ...t.title, program: undefined, activity: undefined },
          }));
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
          if (background()) patch(id, () => ({ attention: { reason: msg.type, at: now, text: msg.type === "notify" ? [msg.title, msg.body].filter(Boolean).join("：") : undefined } }));
          break;
        case "progress":
          patch(id, () => ({ progress: msg.state === 0 ? undefined : { state: msg.state, value: msg.value } }));
          break;
        case "process":
          patch(id, (t) => {
            const kind = kindFromChain(msg.chain);
            return {
              procs: msg.chain.length ? msg.chain : undefined,
              // 进程树认出了更具体的程序（npm run dev → node、脚本里起的 ssh），图标跟着改
              running: t.running && kind && t.running.kind !== kind ? { ...t.running, kind } : t.running,
            };
          });
          break;
        case "git":
          patch(id, () => ({ git: msg.branch ?? undefined }));
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
