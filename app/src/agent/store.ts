// Agent 额度数据的共享状态：浮动卡片、状态栏一段、独立小窗都读这一份，
// 只有一个轮询（App 里启动），不各自向后端要数据。
import { create } from "zustand";
import type { Backend, UsageReport } from "../backend/types";
import { focusedTab, useStore } from "../state/store";
import { agentFocus } from "./usage";

const POLL_MS = 5000;

interface UsageState {
  report: UsageReport | null;
  /** 拿到这份数据时的本地时刻（毫秒），倒计时按它算 */
  fetchedAt: number;
  refreshing: boolean;
  set(report: UsageReport | null): void;
  setRefreshing(on: boolean): void;
}

export const useUsage = create<UsageState>((set) => ({
  report: null,
  fetchedAt: 0,
  refreshing: false,
  set(report) {
    set({ report, fetchedAt: Date.now() });
  },
  setRefreshing(on) {
    set({ refreshing: on });
  },
}));

/** 当前标签页正在跑的 Agent（独立小窗里没有标签页，就不查上下文） */
const currentFocus = () => agentFocus(focusedTab(useStore.getState()));

const focusKey = () => {
  const f = currentFocus();
  return f ? `${f.kind}|${f.tabId}|${f.sinceMs}` : "";
};

/** 开始轮询；返回停止函数。功能关着时不要调用。切换标签页、Agent 启动或退出时立刻刷新一次。 */
export function startUsagePolling(backend: Backend): () => void {
  let live = true;
  const fetchNow = () => {
    backend
      .agentUsage(currentFocus())
      .then((r) => live && useUsage.getState().set(r))
      .catch(() => {});
  };
  fetchNow();
  const t = setInterval(fetchNow, POLL_MS);
  let key = focusKey();
  const unsub = useStore.subscribe(() => {
    const k = focusKey();
    if (k !== key) {
      key = k;
      fetchNow();
    }
  });
  return () => {
    live = false;
    clearInterval(t);
    unsub();
    useUsage.getState().set(null);
  };
}

/** 「刷新」：先让后端立刻查一次实时额度，再拉一次数据 */
export async function refreshUsage(backend: Backend): Promise<string | null> {
  const st = useUsage.getState();
  if (st.refreshing) return null;
  st.setRefreshing(true);
  let error: string | null = null;
  try {
    await backend.agentRefreshLive();
  } catch (e) {
    error = String(e);
  }
  try {
    useUsage.getState().set(await backend.agentUsage(currentFocus()));
  } catch {
    // 下一轮轮询会再试
  }
  useUsage.getState().setRefreshing(false);
  return error;
}
