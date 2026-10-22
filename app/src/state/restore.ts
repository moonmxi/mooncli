// 恢复上次的会话（docs/06 §7）：关窗口时有哪些标签页、各在哪个目录、改过的名字、颜色、置顶、
// 分屏布局，下次打开照原样再开出来（shell 是新开的，屏幕内容和正在跑的程序不恢复）。
import type { PaneNode } from "../layout/layout";
import { newTabId, orderedTabs, panesOf, useStore, type Tab, type TabColor } from "./store";

const KEY = "mooncli.session";
const VERSION = 1;

export interface SavedPane {
  cwd?: string;
}

export interface SavedTab {
  name?: string;
  pinned?: boolean;
  color?: TabColor;
  /** 第一个窗格就是标签页自己 */
  panes: SavedPane[];
  /** 分屏布局，叶子是 panes 的下标 */
  tree?: SavedNode;
  /** 获得焦点的窗格的下标 */
  focus?: number;
}

export type SavedNode = { pane: number } | { dir: "row" | "column"; children: SavedNode[] };

export interface SavedSession {
  version: number;
  tabs: SavedTab[];
  active: number;
}

function saveNode(n: PaneNode, ids: string[]): SavedNode {
  if ("pane" in n) return { pane: ids.indexOf(n.pane) };
  return { dir: n.dir, children: n.children.map((c) => saveNode(c, ids)) };
}

function loadNode(n: SavedNode, ids: string[]): PaneNode | null {
  if ("pane" in n) return ids[n.pane] ? { pane: ids[n.pane] } : null;
  const children = n.children.map((c) => loadNode(c, ids)).filter((c): c is PaneNode => c !== null);
  if (children.length === 0) return null;
  return children.length === 1 ? children[0] : { dir: n.dir, children };
}

/** 当前标签页 → 可保存的样子。只保存本机目录（远程的目录下次开不出来） */
export function snapshot(tabs: Tab[], activeId?: string): SavedSession {
  const list = orderedTabs(tabs);
  return {
    version: VERSION,
    active: Math.max(0, list.findIndex((t) => t.id === activeId)),
    tabs: list.map((t) => {
      const ids = panesOf(t);
      const panes = ids.map((id) => ({ cwd: tabs.find((x) => x.id === id)?.cwd }));
      return {
        name: t.title.user,
        pinned: t.pinned || undefined,
        color: t.color,
        panes,
        tree: t.split ? saveNode(t.split.tree, ids) : undefined,
        focus: t.split ? ids.indexOf(t.split.focus) : undefined,
      };
    }),
  };
}

/** 保存的样子 → 新的标签页（新的会话 id） */
export function rebuild(saved: SavedSession, makeId: () => string = newTabId): { tabs: Tab[]; activeId?: string } {
  const tabs: Tab[] = [];
  const owners: string[] = [];
  for (const st of saved.tabs) {
    if (!st.panes?.length) continue;
    const ids = st.panes.map(() => makeId());
    const tree = st.tree ? loadNode(st.tree, ids) : null;
    const split = tree && "dir" in tree ? { tree, focus: ids[st.focus ?? 0] ?? ids[0] } : undefined;
    st.panes.forEach((p, i) => {
      const base: Tab = { id: ids[i], profile: "日常", title: {}, shellState: "unknown", initialCwd: p.cwd, cwd: p.cwd };
      tabs.push(
        i === 0
          ? { ...base, title: { user: st.name }, pinned: st.pinned, color: st.color, split }
          : { ...base, paneOf: split ? ids[0] : undefined },
      );
    });
    // 布局丢了的话多出来的窗格没地方放，不要
    if (!split) tabs.splice(tabs.length - st.panes.length + 1);
    owners.push(ids[0]);
  }
  return { tabs, activeId: owners[saved.active] ?? owners[0] };
}

export function loadSaved(): SavedSession | null {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null") as SavedSession | null;
    return v && v.version === VERSION && Array.isArray(v.tabs) && v.tabs.length > 0 ? v : null;
  } catch {
    return null;
  }
}

/** 标签页一变就（防抖）存一份 */
export function startSaving(): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let last = "";
  const unsub = useStore.subscribe((s) => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (s.tabs.length === 0) return;
      const text = JSON.stringify(snapshot(s.tabs, s.activeId));
      if (text === last) return;
      last = text;
      try {
        localStorage.setItem(KEY, text);
      } catch {
        // 存不了就算了
      }
    }, 500);
  });
  return () => {
    clearTimeout(timer);
    unsub();
  };
}
