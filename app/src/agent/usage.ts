// Agent 用量小窗的纯函数：标签文案、数字格式、给当前标签页挑会话。
// 数据结构见 backend/types.ts（和 crates/mooncli-agent 一一对应）。
import type { AgentAccountUsage, AgentKind, AgentSessionUsage, SnapshotSource, UsageReport } from "../backend/types";
import type { Tab } from "../state/store";

export const KIND_LABEL: Record<AgentKind, string> = { claude: "Claude Code", codex: "Codex" };
/** 状态栏等窄处用的短名 */
export const KIND_SHORT: Record<AgentKind, string> = { claude: "Claude", codex: "Codex" };

/** 窗口长度 → 显示名。认分钟数不认套餐：300 是 5 小时，10080 是一周，其它照实说。 */
export function windowLabel(windowMinutes?: number): string {
  if (windowMinutes === undefined) return "额度";
  if (windowMinutes === 300) return "5 小时";
  if (windowMinutes === 10080) return "本周";
  if (windowMinutes < 60) return `${windowMinutes} 分钟`;
  if (windowMinutes % 1440 === 0) return `${windowMinutes / 1440} 天`;
  if (windowMinutes % 60 === 0) return `${windowMinutes / 60} 小时`;
  return `${windowMinutes} 分钟`;
}

/** 126000 → "126k"，1200000 → "1.2M" */
export function formatTokens(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) {
    const v = n / 1000;
    return `${v >= 100 ? Math.round(v) : Math.round(v * 10) / 10}k`;
  }
  return `${Math.round((n / 1_000_000) * 10) / 10}M`;
}

/** 距重置还有多久："42 分钟后重置" / "2 小时 18 分后重置" / "3 天后重置" / "已重置" */
export function formatResetIn(resetsAt: number | undefined, nowSec: number): string | null {
  if (resetsAt === undefined) return null;
  const diff = resetsAt - nowSec;
  if (diff <= 0) return "已重置";
  if (diff < 3600) return `${Math.max(1, Math.ceil(diff / 60))} 分钟后重置`;
  if (diff < 48 * 3600) {
    const h = Math.floor(diff / 3600);
    const m = Math.floor((diff % 3600) / 60);
    return m > 0 ? `${h} 小时 ${m} 分后重置` : `${h} 小时后重置`;
  }
  return `${Math.round(diff / 86400)} 天后重置`;
}

export function contextPercent(used: number, window: number): number {
  if (window <= 0) return 0;
  return Math.min(100, Math.max(0, (used / window) * 100));
}

export type Level = "ok" | "warn" | "danger";

/** 比例（0–1）落在哪一档，阈值来自设置 */
export function levelOf(ratio: number, warnAt: number, dangerAt: number): Level {
  if (ratio >= dangerAt) return "danger";
  if (ratio >= warnAt) return "warn";
  return "ok";
}

/** 已运行时长："14m" / "1h 05m" */
export function formatElapsed(sinceMs: number, nowMs: number): string {
  const mins = Math.max(0, Math.floor((nowMs - sinceMs) / 60000));
  if (mins < 60) return `${mins}m`;
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, "0")}m`;
}

const normPath = (p?: string) => (p ?? "").replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

export interface Picked {
  session: AgentSessionUsage;
  /** 怎么挑中的：tab = 就是当前标签页的会话；cwd = 目录对上；recent = 最近活动的一个 */
  via: "tab" | "cwd" | "recent";
}

/** 跟随当前标签（docs/07 第 5 节）：优先标签页自己的会话，其次同目录的，最后最近活动的。 */
export function pickSession(report: UsageReport, tab?: Tab): Picked | null {
  const sessions = report.sessions;
  if (sessions.length === 0) return null;
  if (tab) {
    const own = sessions.find((s) => s.tabSessionId && s.tabSessionId === tab.id);
    if (own) return { session: own, via: "tab" };
    const tabCwd = normPath(tab.remote?.cwd ?? tab.cwd);
    if (tabCwd) {
      const runningKind = tab.running?.kind;
      const sameDir = sessions
        .filter((s) => normPath(s.cwd) === tabCwd)
        .sort((a, b) => b.updatedAt - a.updatedAt);
      const match = sameDir.find((s) => s.kind === runningKind) ?? sameDir[0];
      if (match) return { session: match, via: "cwd" };
    }
  }
  const freshest = [...sessions].sort((a, b) => b.updatedAt - a.updatedAt)[0];
  return { session: freshest, via: "recent" };
}

export function accountFor(report: UsageReport, kind: AgentKind): AgentAccountUsage | undefined {
  return report.accounts.find((a) => a.kind === kind);
}

/** 数据来源的说明文字（小窗底部的小字） */
export function sourceLabel(source: SnapshotSource): string {
  switch (source) {
    case "live":
      return "实时 · codex app-server";
    case "statusLine":
      return "实时 · Claude Code statusLine";
    case "sessionFile":
      return "会话文件里的最近记录";
  }
}

/** 窗口过了重置点还没新数据：按已重置算 0%（官方此时也不再下发旧值） */
export function shownPercent(w: { usedPercent: number; resetsAt?: number }, nowSec: number): number {
  return w.resetsAt !== undefined && w.resetsAt <= nowSec ? 0 : w.usedPercent;
}

/** 状态栏的一段摘要："5h 38% · 周 21%"；两个窗口都没有时为空 */
export function compactSummary(a: AgentAccountUsage | undefined, nowSec: number): string | null {
  if (!a) return null;
  const parts: string[] = [];
  for (const w of [a.primary, a.secondary]) {
    if (!w) continue;
    const name = w.windowMinutes === 300 ? "5h" : w.windowMinutes === 10080 ? "周" : windowLabel(w.windowMinutes);
    parts.push(`${name} ${Math.round(shownPercent(w, nowSec))}%`);
  }
  return parts.length ? parts.join(" · ") : null;
}

/** 当前该看哪个 Agent 的账号：跟随挑中的会话；没有会话时取最新的账号数据 */
export function pickAccount(report: UsageReport, session?: AgentSessionUsage): AgentAccountUsage | undefined {
  if (session) return accountFor(report, session.kind);
  return [...report.accounts].sort((a, b) => b.updatedAt - a.updatedAt)[0];
}
