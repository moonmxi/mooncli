// Agent 用量数据的共享状态：浮动卡片、侧栏区块、状态栏一段、独立小窗都读这一份，
// 只有一个轮询（App 里启动），不各自向后端要数据。
import { create } from "zustand";
import type { Backend, UsageReport } from "../backend/types";

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

/** 开始轮询；返回停止函数。功能关着时不要调用。 */
export function startUsagePolling(backend: Backend): () => void {
  let live = true;
  const fetchNow = () => {
    backend
      .agentUsage()
      .then((r) => live && useUsage.getState().set(r))
      .catch(() => {});
  };
  fetchNow();
  const t = setInterval(fetchNow, POLL_MS);
  return () => {
    live = false;
    clearInterval(t);
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
    useUsage.getState().set(await backend.agentUsage());
  } catch {
    // 下一轮轮询会再试
  }
  useUsage.getState().setRefreshing(false);
  return error;
}
