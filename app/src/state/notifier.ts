// 「等你操作」提醒（docs/06 §5）：后台标签页需要你看一眼时，标签上亮一个点；窗口不在前台时
// 再让任务栏图标闪烁、发一条系统通知，点通知回到那个标签页。
// Agent 的完成提醒（docs/07 第 7 节）由 mooncli-bridge 交来：一轮结束、要你确认时按你在不在看决定怎么提醒。
import type { AgentAlert, Backend } from "../backend/types";
import { useSettings } from "./settings";
import { focusedTab, listView, useStore, type Tab } from "./store";
import { displayTitle } from "./title";
import { useUi } from "./ui";

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

export type AlertAction = "none" | "toast" | "inApp";

/**
 * Agent 提醒怎么发：
 * - 正看着那个标签页（窗口在前台、焦点就在这个窗格）：开了「正看着时不提醒」就不打扰，否则照样弹；
 * - 窗口在前台但在看别的标签页：标签上亮点 + 窗口里一条提示，不弹系统通知；
 * - 窗口不在前台：弹系统通知。
 */
export function alertAction(watching: boolean, windowFocused: boolean, quietWhenFocused: boolean): AlertAction {
  if (watching) return quietWhenFocused ? "none" : "toast";
  return windowFocused ? "inApp" : "toast";
}

function handleAlert(backend: Backend, alert: AgentAlert) {
  const st = useStore.getState();
  const prefs = useSettings.getState().settings.agent.notify;
  if (!prefs.enabled) return;
  const tabId = alert.tabSessionId && st.tabs.some((t) => t.id === alert.tabSessionId) ? alert.tabSessionId : undefined;
  const watching = !!tabId && st.focused && focusedTab(st)?.id === tabId;
  const action = alertAction(watching, st.focused, prefs.quietWhenFocused);
  if (tabId) st.flag(tabId, alert.event === "waiting" ? "agent-waiting" : "agent-done", `${alert.title}：${alert.body}`);
  if (action === "inApp") useUi.getState().showNotice(alert.body ? `${alert.title}：${alert.body}` : alert.title);
  if (action === "toast") backend.attention(alert.title, alert.body, true, tabId);
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
      // Agent 的提醒在 handleAlert 里已经发过了
      if (a.reason === "agent-done" || a.reason === "agent-waiting") continue;
      const n = useSettings.getState().settings.notifications;
      // 窗口在前台时只在标签上亮点，不闪任务栏、不弹通知
      if (s.focused || (!n.taskbar && !n.system)) continue;
      const owner = s.tabs.find((x) => x.id === (t.paneOf ?? t.id));
      const view = owner ? { ...listView(s.tabs, owner), attention: a, lastCommand: t.lastCommand } : t;
      const msg = attentionMessage(view);
      backend.attention(msg.title, msg.body, n.system, t.id);
    }
  });
  const offAlert = backend.onAgentAlert((a) => handleAlert(backend, a));
  // 点了通知：切到那个标签页（窗口已经由后端拉到前面）
  const offClick = backend.onToastActivated((tabId) => {
    if (useStore.getState().tabs.some((t) => t.id === tabId)) useStore.getState().activate(tabId);
  });
  return () => {
    unsubStore();
    offAlert();
    offClick();
  };
}
