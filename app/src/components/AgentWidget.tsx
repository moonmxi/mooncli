// Agent 用量小窗（docs/07 第 5 节）：跟随当前标签页，显示对应 Agent（Claude Code / Codex）的
// 上下文占用与账号额度（5 小时 / 周）。四种形态共用一张卡片：
//   浮动卡片（可拖动，松手吸附到最近的角）、侧栏底部区块、状态栏一段（StatusSegment）、
//   独立置顶小窗（AgentWindowView，单独的 Tauri 窗口）。
import { useEffect, useRef, useState } from "react";
import type { AgentSessionUsage, Backend, RateWindow, UsageReport } from "../backend/types";
import {
  compactSummary,
  contextPercent,
  formatElapsed,
  formatResetIn,
  formatTokens,
  KIND_LABEL,
  KIND_SHORT,
  levelOf,
  pickAccount,
  pickSession,
  shownPercent,
  sourceLabel,
  windowLabel,
  type Level,
} from "../agent/usage";
import { refreshUsage, useUsage } from "../agent/store";
import { useSettings, type AppSettings } from "../state/settings";
import { useStore, type Tab } from "../state/store";
import { useUi } from "../state/ui";

type Anchor = AppSettings["agent"]["widget"]["anchor"];

function Bar({ ratio, level }: { ratio: number; level: Level }) {
  return (
    <span className={`aw-bar ${level}`}>
      <i style={{ width: `${Math.min(100, Math.max(0, ratio * 100))}%` }} />
    </span>
  );
}

function WindowRow({ w, nowSec, warnAt, dangerAt }: { w: RateWindow; nowSec: number; warnAt: number; dangerAt: number }) {
  // 窗口已过重置点但还没新数据：官方此时已不再下发旧百分比，显示陈旧值会误导——按已重置呈现
  const pct = shownPercent(w, nowSec);
  const reset = formatResetIn(w.resetsAt, nowSec);
  const level = levelOf(pct / 100, warnAt, dangerAt);
  return (
    <div className="aw-row">
      <span className="aw-name">{windowLabel(w.windowMinutes)}</span>
      <Bar ratio={pct / 100} level={level} />
      <span className={`aw-num ${level}`} title={w.resetsAt ? new Date(w.resetsAt * 1000).toLocaleString() : undefined}>
        {Math.round(pct)}%{reset ? ` · ${reset.replace("后重置", "")}` : ""}
      </span>
    </div>
  );
}

const STATE_TEXT = { running: "运行中", waiting: "等你确认", done: "本轮已完成", unknown: "" } as const;

/** 当前要显示的会话和账号；没有任何数据时为 null */
function useShown(report: UsageReport | null, tab: Tab | undefined, overrideId: string | null) {
  if (!report) return null;
  const picked = pickSession(report, tab);
  const session: AgentSessionUsage | undefined =
    (overrideId && report.sessions.find((s) => s.sessionId === overrideId)) || picked?.session;
  const account = pickAccount(report, session);
  if (!session && !account) return null;
  return { session, account, via: overrideId ? "tab" : picked?.via };
}

interface CardProps {
  backend: Backend;
  /** 卡片头部可拖动（浮动形态） */
  onDragStart?: (e: React.PointerEvent) => void;
}

/** 卡片本体：各形态共用 */
export function UsageCard({ backend, onDragStart }: CardProps) {
  const settings = useSettings((s) => s.settings);
  const report = useUsage((s) => s.report);
  const fetchedAt = useUsage((s) => s.fetchedAt);
  const refreshing = useUsage((s) => s.refreshing);
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const activate = useStore((s) => s.activate);
  const [overrideId, setOverrideId] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [, tick] = useState(0);
  useEffect(() => setOverrideId(null), [activeId]);
  // 倒计时每 30 秒走一次，不等数据轮询
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 30_000);
    return () => clearInterval(t);
  }, []);

  const tab = tabs.find((t) => t.id === activeId);
  const shown = useShown(report, tab, overrideId);
  if (!report || !shown) {
    return (
      <div className="aw-empty">
        还没有 Claude Code 或 Codex 的用量数据。用它们跑一轮对话后就会出现；接入 statusLine / notify 后更实时（设置 → Agent 用量）。
      </div>
    );
  }
  const { session, account } = shown;
  const { warnAt, dangerAt } = settings.agent.widget;
  const nowSec = Math.floor(Math.max(Date.now(), fetchedAt) / 1000);
  const kind = session?.kind ?? account!.kind;

  const ctx = session?.context;
  const ctxRatio = ctx ? contextPercent(ctx.usedTokens, ctx.windowTokens) / 100 : 0;
  const ctxLevel = levelOf(ctxRatio, warnAt, dangerAt);
  const others = report.sessions.filter((s) => s.sessionId !== session?.sessionId);
  const isOwnTab = session?.tabSessionId && session.tabSessionId === activeId;
  // 只有 Agent 正在跑时才显示已运行多久（Agent 进程本身一直在标签页里跑）
  const runningHere = isOwnTab && session?.state === "running" ? tab?.running : undefined;
  const updatedAt = Math.max(session?.updatedAt ?? 0, account?.updatedAt ?? 0);
  const staleMin = Math.floor((nowSec - updatedAt) / 60);
  const otherAccounts = report.accounts.filter((a) => a.kind !== kind && (a.primary || a.secondary));
  const canRefresh = settings.agent.liveQuota && (kind === "codex" || report.accounts.some((a) => a.kind === "codex"));

  return (
    <>
      <header className={`aw-head${onDragStart ? " draggable" : ""}`} onPointerDown={onDragStart}>
        <strong>
          {KIND_LABEL[kind]}
          {session?.model ? <span className="aw-model"> · {session.model}</span> : null}
          {account?.plan ? <span className="aw-plan">{account.plan}</span> : null}
        </strong>
        {others.length > 0 ? (
          <button type="button" className="aw-all" onPointerDown={(e) => e.stopPropagation()} onClick={() => setShowAll((v) => !v)}>
            全部 {report.sessions.length} 个 {showAll ? "▴" : "▾"}
          </button>
        ) : (
          <span className="aw-follow">{!session ? "账号额度" : shown.via === "recent" ? "最近活动" : "跟随当前标签"}</span>
        )}
      </header>

      {ctx && (
        <div className="aw-row">
          <span className="aw-name">上下文</span>
          <Bar ratio={ctxRatio} level={ctxLevel} />
          <span className={`aw-num ${ctxLevel}`}>
            {Math.round(ctxRatio * 100)}% · {formatTokens(ctx.usedTokens)}/{formatTokens(ctx.windowTokens)}
          </span>
        </div>
      )}
      {ctx && ctxRatio >= 0.85 && <div className="aw-hint">上下文快满了，可以考虑 /compact</div>}

      {account && (account.primary || account.secondary) ? (
        <>
          <div className="aw-divider" />
          {account.primary && <WindowRow w={account.primary} nowSec={nowSec} warnAt={warnAt} dangerAt={dangerAt} />}
          {account.secondary && <WindowRow w={account.secondary} nowSec={nowSec} warnAt={warnAt} dangerAt={dangerAt} />}
        </>
      ) : kind === "claude" ? (
        <div className="aw-note">额度需要接入 statusLine，且仅 Pro / Max 订阅在首次响应后提供</div>
      ) : null}

      {session && session.state !== "unknown" && (
        <div className="aw-state">
          <span className={`aw-dot ${session.state}`} />
          <span>
            {STATE_TEXT[session.state]}
            {runningHere ? ` · ${formatElapsed(runningHere.since, Date.now())}` : ""}
          </span>
        </div>
      )}

      {otherAccounts.map((a) => (
        <div key={a.kind} className="aw-mini">
          <span>{KIND_SHORT[a.kind]}</span>
          <span className="aw-num">{compactSummary(a, nowSec)}</span>
        </div>
      ))}

      <footer className="aw-foot">
        <span title={account ? sourceLabel(account.source) : undefined}>
          {staleMin >= 10
            ? `${new Date(updatedAt * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })} 的记录`
            : account
              ? sourceLabel(account.source)
              : session
                ? sourceLabel(session.source)
                : ""}
        </span>
        {canRefresh && (
          <button
            type="button"
            className="aw-refresh"
            disabled={refreshing}
            title={report.liveError ? `上次查询失败：${report.liveError}` : "通过 codex app-server 重新查询额度"}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => refreshUsage(backend).then((err) => err && useUi.getState().showNotice(`刷新额度失败：${err}`))}
          >
            {refreshing ? "查询中…" : "刷新"}
          </button>
        )}
      </footer>

      {showAll && others.length > 0 && (
        <div className="aw-others">
          {others.map((s) => (
            <button
              key={s.sessionId}
              type="button"
              className="aw-other"
              onClick={() => {
                setOverrideId(s.sessionId);
                if (s.tabSessionId && useStore.getState().tabs.some((t) => t.id === s.tabSessionId)) activate(s.tabSessionId);
              }}
            >
              <span>
                {KIND_SHORT[s.kind]}
                {s.model ? ` · ${s.model}` : ""}
                {s.cwd ? <span className="aw-model"> · {s.cwd.split(/[\\/]/).filter(Boolean).pop()}</span> : null}
              </span>
              <span className="aw-num">{s.context ? `${Math.round(contextPercent(s.context.usedTokens, s.context.windowTokens))}%` : ""}</span>
            </button>
          ))}
        </div>
      )}
    </>
  );
}

/** 拖动结束时离哪个角最近 */
export function nearestAnchor(cx: number, cy: number, vw: number, vh: number): Anchor {
  const v = cy < vh / 2 ? "top" : "bottom";
  const h = cx < vw / 2 ? "left" : "right";
  return `${v}-${h}` as Anchor;
}

/** 浮动卡片：停在四个角之一；按住头部拖动，松手吸附到最近的角 */
export function AgentWidget({ backend }: { backend: Backend }) {
  const settings = useSettings((s) => s.settings);
  // 浮层打开时让位（命令面板、历史面板、其它设置节）；在 Agent 设置节里保留，方便调外观时实时预览
  const overlayOpen = useUi((s) => s.paletteOpen || s.historyOpen || (s.settings.open && s.settings.section !== "agent"));
  const hidden = useUi((s) => s.agentHidden);
  const report = useUsage((s) => s.report);
  const ref = useRef<HTMLElement>(null);
  const [drag, setDrag] = useState<{ x: number; y: number } | null>(null);

  const { enabled, widget } = settings.agent;
  if (!enabled || widget.style !== "floating" || !report || overlayOpen || hidden) return null;
  if (report.sessions.length === 0 && report.accounts.length === 0) return null;

  const onDragStart = (e: React.PointerEvent) => {
    if (e.button !== 0 || !ref.current) return;
    const box = ref.current.getBoundingClientRect();
    const dx = e.clientX - box.left;
    const dy = e.clientY - box.top;
    const move = (ev: PointerEvent) => setDrag({ x: ev.clientX - dx, y: ev.clientY - dy });
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDrag(null);
      const anchor = nearestAnchor(ev.clientX - dx + box.width / 2, ev.clientY - dy + box.height / 2, innerWidth, innerHeight);
      if (anchor !== useSettings.getState().settings.agent.widget.anchor) {
        useSettings.getState().update((d) => (d.agent.widget.anchor = anchor));
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <section
      ref={ref}
      className={`agent-widget${drag ? " dragging" : ""}`}
      data-anchor={drag ? undefined : widget.anchor}
      style={{ opacity: widget.opacity, ...(drag ? { left: drag.x, top: drag.y } : {}) }}
      aria-label="Agent 用量"
    >
      <UsageCard backend={backend} onDragStart={onDragStart} />
    </section>
  );
}

/** 侧栏底部的区块形态 */
export function AgentSidebarBlock({ backend }: { backend: Backend }) {
  const settings = useSettings((s) => s.settings);
  const report = useUsage((s) => s.report);
  if (!settings.agent.enabled || settings.agent.widget.style !== "sidebar" || !report) return null;
  if (report.sessions.length === 0 && report.accounts.length === 0) return null;
  return (
    <section className="agent-block" aria-label="Agent 用量">
      <UsageCard backend={backend} />
    </section>
  );
}

/** 状态栏里的一段：当前标签对应 Agent 的额度摘要。点一下显示 / 隐藏浮动卡片 */
export function StatusSegment() {
  const settings = useSettings((s) => s.settings);
  const report = useUsage((s) => s.report);
  const fetchedAt = useUsage((s) => s.fetchedAt);
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  if (!settings.agent.enabled || !settings.agent.widget.statusBar || !report) return null;
  const tab = tabs.find((t) => t.id === activeId);
  const shown = useShown(report, tab, null);
  if (!shown?.account) return null;
  const nowSec = Math.floor(Math.max(Date.now(), fetchedAt) / 1000);
  const text = compactSummary(shown.account, nowSec);
  if (!text) return null;
  const { warnAt, dangerAt } = settings.agent.widget;
  const worst = Math.max(...[shown.account.primary, shown.account.secondary].filter((w): w is RateWindow => !!w).map((w) => shownPercent(w, nowSec)));
  return (
    <button
      type="button"
      className={`status-btn status-agent ${levelOf(worst / 100, warnAt, dangerAt)}`}
      title={settings.agent.widget.style === "floating" ? "显示 / 隐藏用量小窗" : "Agent 用量"}
      onClick={() => useUi.getState().toggleAgentHidden()}
    >
      {KIND_SHORT[shown.account.kind]} {text}
    </button>
  );
}

/** 独立置顶小窗（?view=agent）：整个窗口就是一张卡片 */
export function AgentWindowView({ backend, onClose }: { backend: Backend; onClose(): void }) {
  return (
    <div className="agent-window" data-tauri-drag-region>
      <button type="button" className="aw-close" title="关闭" onClick={onClose}>
        ×
      </button>
      <UsageCard backend={backend} />
    </div>
  );
}
