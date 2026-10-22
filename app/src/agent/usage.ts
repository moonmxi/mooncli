// Agent 额度卡片的纯函数：标签文案、数字格式、当前标签页在跑哪个 Agent。
// 数据结构见 backend/types.ts（和 crates/mooncli-agent 一一对应）。
import type { AgentAccountUsage, AgentFocus, AgentKind, RateWindow, UsageReport } from "../backend/types";
import type { Tab } from "../state/store";

export const KIND_LABEL: Record<AgentKind, string> = { claude: "Claude Code", codex: "Codex" };
/** 状态栏等窄处用的短名 */
export const KIND_SHORT: Record<AgentKind, string> = { claude: "Claude", codex: "Codex" };
export const KINDS: AgentKind[] = ["claude", "codex"];

/** 126000 → "126k"，1200000 → "1.2M" */
export function formatTokens(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) {
    const v = n / 1000;
    return `${v >= 100 ? Math.round(v) : Math.round(v * 10) / 10}k`;
  }
  return `${Math.round((n / 1_000_000) * 10) / 10}M`;
}

/** 距重置还有多久："42 分钟" / "2 小时 18 分" / "3 天"；过了重置点为空 */
export function formatResetIn(resetsAt: number | undefined, nowSec: number): string | null {
  if (resetsAt === undefined) return null;
  const diff = resetsAt - nowSec;
  if (diff <= 0) return null;
  if (diff < 3600) return `${Math.max(1, Math.ceil(diff / 60))} 分钟`;
  if (diff < 48 * 3600) {
    const h = Math.floor(diff / 3600);
    const m = Math.floor((diff % 3600) / 60);
    return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`;
  }
  return `${Math.round(diff / 86400)} 天`;
}

export type Level = "ok" | "warn" | "danger";

/** 比例（0–1）落在哪一档，阈值来自设置 */
export function levelOf(ratio: number, warnAt: number, dangerAt: number): Level {
  if (ratio >= dangerAt) return "danger";
  if (ratio >= warnAt) return "warn";
  return "ok";
}

/** 窗口过了重置点还没新数据：按已重置算 0%（官方此时也不再下发旧值） */
export function shownPercent(w: RateWindow, nowSec: number): number {
  return w.resetsAt !== undefined && w.resetsAt <= nowSec ? 0 : w.usedPercent;
}

export function accountFor(report: UsageReport | null, kind: AgentKind): AgentAccountUsage | undefined {
  return report?.accounts.find((a) => a.kind === kind);
}

/** 当前标签页（获得焦点的窗格）正在本机跑 Claude Code 或 Codex 时，返回要查上下文的对象 */
export function agentFocus(tab: Tab | undefined): AgentFocus | undefined {
  const r = tab?.running;
  if (!tab || !r || r.remote || tab.remote || (r.kind !== "claude" && r.kind !== "codex")) return undefined;
  return { kind: r.kind, tabId: tab.id, cwd: tab.cwd, sinceMs: r.since };
}

/** 状态栏的一段："Claude 46% · 6%"（5 小时 · 7 天）；没有数据时为空 */
export function compactSummary(a: AgentAccountUsage | undefined, nowSec: number): string | null {
  if (!a) return null;
  const pct = (w?: RateWindow) => (w ? `${Math.round(shownPercent(w, nowSec))}%` : "–");
  return `${KIND_SHORT[a.kind]} ${pct(a.fiveHour)} · ${pct(a.sevenDay)}`;
}

/** 两个窗口里最紧张的那个（状态栏配色用） */
export function worstPercent(accounts: AgentAccountUsage[], nowSec: number): number {
  const all = accounts.flatMap((a) => [a.fiveHour, a.sevenDay]).filter((w): w is RateWindow => !!w);
  return all.length ? Math.max(...all.map((w) => shownPercent(w, nowSec))) : 0;
}
