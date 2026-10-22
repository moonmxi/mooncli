// 「等你操作」提醒（docs/06 §5）：后台标签页需要你看一眼时，标签上亮一个点；窗口不在前台时
// 再让任务栏图标闪烁、发一条系统通知。
// Agent 的状态（等你确认、本轮已完成）来自用量数据里 hooks / notify 报来的会话状态。
import type { AgentSessionState, Backend, UsageReport } from "../backend/types";
import { KIND_LABEL } from "../agent/usage";
import { useUsage } from "../agent/store";
import { useSettings } from "./settings";
import { listView, useStore, type Tab } from "./store";
import { displayTitle } from "./title";

const REASON_TEXT: Record<NonNullable<Tab["attention"]>["reason"], string> = {
  "agent-waiting": "在等你确认",
  "agent-done": "本轮已完成",
  bell: "响铃了",
  notify: "发来通知",
  "long-command": "命令跑完了",
};

/** 一条提醒的通知文字 */
export function attentionMessage(tab: Tab): { title: string; body: string } {
  const a = tab.attention!;
  const title = `mooncli · ${displayTitle(tab)}`;
  if (a.text) return { title, body: a.text };
  if (a.reason === "long-command" && tab.lastCommand) {
    const ok = tab.lastCommand.exitCode === 0;
    return { title, body: `${ok ? "完成" : "失败"}：${tab.lastCommand.text ?? "命令"}` };
  }
  return { title, body: REASON_TEXT[a.reason] };
}

/** Agent 会话状态的变化 → 该提醒的标签页：变成「等你确认」或「本轮已完成」的、关联了标签页的会话 */
export function agentTransitions(
  before: Map<string, AgentSessionState>,
  report: UsageReport,
): { tabId: string; reason: "agent-waiting" | "agent-done"; text: string }[] {
  const out: { tabId: string; reason: "agent-waiting" | "agent-done"; text: string }[] = [];
  for (const s of report.sessions) {
    const prev = before.get(s.sessionId);
    if (prev === undefined || prev === s.state || !s.tabSessionId) continue;
    if (s.state === "waiting") out.push({ tabId: s.tabSessionId, reason: "agent-waiting", text: `${KIND_LABEL[s.kind]} 在等你确认` });
    if (s.state === "done") out.push({ tabId: s.tabSessionId, reason: "agent-done", text: `${KIND_LABEL[s.kind]} 本轮已完成` });
  }
  return out;
}

/** 开始盯着提醒；返回停止函数 */
export function startNotifier(backend: Backend): () => void {
  const seen = new Map<string, number>();
  const unsubStore = useStore.subscribe((s) => {
    for (const t of s.tabs) {
      const a = t.attention;
      if (!a) {
        seen.delete(t.id);
        continue;
      }
      if (seen.get(t.id) === a.at) continue;
      seen.set(t.id, a.at);
      const n = useSettings.getState().settings.notifications;
      // 窗口在前台时只在标签上亮点，不闪任务栏、不弹通知
      if (s.focused || (!n.taskbar && !n.system)) continue;
      const owner = s.tabs.find((x) => x.id === (t.paneOf ?? t.id));
      const view = owner ? { ...listView(s.tabs, owner), attention: a, lastCommand: t.lastCommand } : t;
      const msg = attentionMessage(view);
      backend.attention(msg.title, msg.body, n.system);
    }
  });
  let states = new Map<string, AgentSessionState>();
  const unsubUsage = useUsage.subscribe((u) => {
    if (!u.report) return;
    if (states.size > 0) {
      for (const tr of agentTransitions(states, u.report)) useStore.getState().flag(tr.tabId, tr.reason, tr.text);
    }
    states = new Map(u.report.sessions.map((s) => [s.sessionId, s.state]));
  });
  return () => {
    unsubStore();
    unsubUsage();
  };
}
