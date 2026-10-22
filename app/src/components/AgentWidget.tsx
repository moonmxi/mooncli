// Agent 额度卡片（docs/07 第 5 节）：固定列出 Claude Code 和 Codex 各自的 5 小时、7 天额度；
// 当前标签页正在跑其中一个时，再加一行这个会话的上下文占用。三种形态共用一张卡片：
//   浮动卡片（可拖动，松手吸附到最近的角）、状态栏一段（StatusSegment）、独立置顶小窗（AgentWindowView）。
import { useEffect, useRef, useState } from "react";
import type { AgentAccountUsage, AgentKind, Backend, RateWindow } from "../backend/types";
import { accountFor, compactSummary, formatResetIn, formatTokens, KIND_LABEL, KINDS, levelOf, shownPercent, worstPercent, type Level } from "../agent/usage";
import { refreshUsage, useUsage } from "../agent/store";
import { useSettings, type AppSettings } from "../state/settings";
import { useUi } from "../state/ui";

type Anchor = AppSettings["agent"]["widget"]["anchor"];

function Bar({ ratio, level }: { ratio: number; level: Level }) {
  return (
    <span className={`aw-bar ${level}`}>
      <i style={{ width: `${Math.min(100, Math.max(0, ratio * 100))}%` }} />
    </span>
  );
}

function WindowRow({ name, w, nowSec }: { name: string; w?: RateWindow; nowSec: number }) {
  const { warnAt, dangerAt } = useSettings((s) => s.settings.agent.widget);
  if (!w) {
    return (
      <div className="aw-row">
        <span className="aw-name">{name}</span>
        <Bar ratio={0} level="ok" />
        <span className="aw-num aw-muted">–</span>
      </div>
    );
  }
  // 过了重置点还没新数据：官方此时已不再下发旧百分比，按已重置呈现
  const pct = shownPercent(w, nowSec);
  const reset = formatResetIn(w.resetsAt, nowSec);
  const level = levelOf(pct / 100, warnAt, dangerAt);
  return (
    <div className="aw-row">
      <span className="aw-name">{name}</span>
      <Bar ratio={pct / 100} level={level} />
      <span className="aw-num" title={w.resetsAt ? `${new Date(w.resetsAt * 1000).toLocaleString()} 重置` : undefined}>
        <b className={level}>{Math.round(pct)}%</b>
        {reset && <span className="aw-reset">{reset}</span>}
      </span>
    </div>
  );
}

/** 数据有多旧：十分钟内不提，更久的标出时刻 */
function staleText(updatedAt: number, nowSec: number): string | null {
  if (nowSec - updatedAt < 600) return null;
  const d = new Date(updatedAt * 1000);
  const sameDay = d.toDateString() === new Date(nowSec * 1000).toDateString();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return sameDay ? `${time} 的数据` : `${d.getMonth() + 1}/${d.getDate()} ${time} 的数据`;
}

function AgentBlock({ kind, account, nowSec, missing }: { kind: AgentKind; account?: AgentAccountUsage; nowSec: number; missing: string }) {
  const stale = account ? staleText(account.updatedAt, nowSec) : null;
  return (
    <div className="aw-agent">
      <div className="aw-agent-head">
        <span>
          {KIND_LABEL[kind]}
          {account?.plan && <span className="aw-plan">{account.plan}</span>}
        </span>
        {stale && <span className="aw-stale">{stale}</span>}
      </div>
      {account ? (
        <>
          <WindowRow name="5 小时" w={account.fiveHour} nowSec={nowSec} />
          <WindowRow name="7 天" w={account.sevenDay} nowSec={nowSec} />
        </>
      ) : (
        <div className="aw-note">{missing}</div>
      )}
    </div>
  );
}

interface CardProps {
  backend: Backend;
  /** 卡片头部可拖动（浮动形态） */
  onDragStart?: (e: React.PointerEvent) => void;
}

/** 卡片本体：各形态共用 */
export function UsageCard({ backend, onDragStart }: CardProps) {
  const report = useUsage((s) => s.report);
  const fetchedAt = useUsage((s) => s.fetchedAt);
  const refreshing = useUsage((s) => s.refreshing);
  const { warnAt, dangerAt } = useSettings((s) => s.settings.agent.widget);
  const liveQuota = useSettings((s) => s.settings.agent.liveQuota);
  const [, tick] = useState(0);
  // 倒计时每 30 秒走一次，不等数据轮询
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 30_000);
    return () => clearInterval(t);
  }, []);
  const nowSec = Math.floor(Math.max(Date.now(), fetchedAt) / 1000);
  const missing: Record<AgentKind, string> = {
    claude: report?.claudeIntegrated
      ? "等 Claude Code 下一次回复后显示（Pro / Max 订阅才有额度）"
      : "接入 Claude Code 后显示（设置 → Agent）",
    codex: report?.liveError ? `没查到：${report.liveError}` : "还没有 Codex 的额度数据",
  };
  const current = report?.current;
  const ctx = current?.context;
  const ctxLevel = ctx ? levelOf(ctx.usedPercent / 100, warnAt, dangerAt) : "ok";

  return (
    <>
      <header className={`aw-head${onDragStart ? " draggable" : ""}`} onPointerDown={onDragStart}>
        <strong>Agent 额度</strong>
        {liveQuota && (
          <button
            type="button"
            className="aw-refresh"
            disabled={refreshing}
            title={report?.liveError ? `上次查询失败：${report.liveError}` : "通过 codex app-server 重新查询 Codex 额度"}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => refreshUsage(backend).then((err) => err && useUi.getState().showNotice(`刷新额度失败：${err}`))}
          >
            {refreshing ? "查询中…" : "刷新"}
          </button>
        )}
      </header>
      {KINDS.map((kind) => (
        <AgentBlock key={kind} kind={kind} account={accountFor(report, kind)} nowSec={nowSec} missing={missing[kind]} />
      ))}
      {current && (
        <div className="aw-current">
          <div className="aw-agent-head">
            <span>
              当前会话 · {KIND_LABEL[current.kind]}
              {current.model && <span className="aw-model"> · {current.model}</span>}
            </span>
          </div>
          {ctx ? (
            <div className="aw-row">
              <span className="aw-name">上下文</span>
              <Bar ratio={ctx.usedPercent / 100} level={ctxLevel} />
              <span className="aw-num">
                <b className={ctxLevel}>{Math.round(ctx.usedPercent)}%</b>
                <span className="aw-reset">
                  {formatTokens(ctx.usedTokens)}/{formatTokens(ctx.windowTokens)}
                </span>
              </span>
            </div>
          ) : (
            <div className="aw-note">第一轮回复后显示上下文占用</div>
          )}
          {ctx && ctx.usedPercent >= 85 && (
            <div className="aw-hint">{current.kind === "claude" ? "上下文快满了，可以 /compact" : "上下文快满了，可以 /compact 或开新会话"}</div>
          )}
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
      aria-label="Agent 额度"
    >
      <UsageCard backend={backend} onDragStart={onDragStart} />
    </section>
  );
}

/** 状态栏里的一段：两个 Agent 的 5 小时 · 7 天额度。点一下显示 / 隐藏浮动卡片 */
export function StatusSegment() {
  const settings = useSettings((s) => s.settings);
  const report = useUsage((s) => s.report);
  const fetchedAt = useUsage((s) => s.fetchedAt);
  if (!settings.agent.enabled || !settings.agent.widget.statusBar || !report || report.accounts.length === 0) return null;
  const nowSec = Math.floor(Math.max(Date.now(), fetchedAt) / 1000);
  const text = KINDS.map((k) => compactSummary(accountFor(report, k), nowSec))
    .filter(Boolean)
    .join("   ");
  const { warnAt, dangerAt } = settings.agent.widget;
  return (
    <button
      type="button"
      className={`status-btn status-agent ${levelOf(worstPercent(report.accounts, nowSec) / 100, warnAt, dangerAt)}`}
      title={settings.agent.widget.style === "floating" ? "Agent 额度（5 小时 · 7 天），点击显示 / 隐藏卡片" : "Agent 额度（5 小时 · 7 天）"}
      onClick={() => useUi.getState().toggleAgentHidden()}
    >
      {text}
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
