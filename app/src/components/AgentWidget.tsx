// Agent 用量小窗（docs/07 第 5 节）：浮动卡片，跟随当前标签页显示对应 Agent 的
// 上下文占用与账号额度（5 小时 / 周）。数据每 5 秒向后端要一次（后端只读本地文件与 spool）。
import { useEffect, useState } from "react";
import type { Backend, RateWindow } from "../backend/types";
import { accountFor, contextPercent, formatElapsed, formatResetIn, formatTokens, KIND_LABEL, levelOf, pickSession, windowLabel, type Level } from "../agent/usage";
import { useSettings } from "../state/settings";
import { useStore } from "../state/store";
import { useUi } from "../state/ui";

const POLL_MS = 5000;

function Bar({ ratio, level }: { ratio: number; level: Level }) {
  return (
    <span className={`aw-bar ${level}`}>
      <i style={{ width: `${Math.min(100, Math.max(0, ratio * 100))}%` }} />
    </span>
  );
}

function WindowRow({ label, w, nowSec, warnAt, dangerAt }: { label: string; w: RateWindow; nowSec: number; warnAt: number; dangerAt: number }) {
  // 窗口已过重置点但还没新数据：官方此时已不再下发旧百分比，显示陈旧值会误导——按已重置呈现
  const resetPassed = w.resetsAt !== undefined && w.resetsAt <= nowSec;
  const shownPercent = resetPassed ? 0 : w.usedPercent;
  const reset = resetPassed ? "已重置" : formatResetIn(w.resetsAt, nowSec);
  const level = levelOf(shownPercent / 100, warnAt, dangerAt);
  return (
    <>
      <span className="aw-name">{label}</span>
      <Bar ratio={shownPercent / 100} level={level} />
      <span className={`aw-num ${level}`}>
        {Math.round(shownPercent)}%{reset ? ` · ${reset.replace("后重置", "")}` : ""}
      </span>
    </>
  );
}

const STATE_TEXT = { running: "运行中", waiting: "等待你确认", done: "本轮已完成", unknown: "" } as const;

export function AgentWidget({ backend }: { backend: Backend }) {
  const settings = useSettings((s) => s.settings);
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const activate = useStore((s) => s.activate);
  // 浮层打开时让位（命令面板、历史面板、其它设置节）；在 Agent 设置节里保留，方便调外观时实时预览
  const overlayOpen = useUi((s) => s.paletteOpen || s.historyOpen || (s.settings.open && s.settings.section !== "agent"));
  const [report, setReport] = useState<Awaited<ReturnType<Backend["agentUsage"]>> | null>(null);
  const [overrideId, setOverrideId] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());

  const enabled = settings.agent.enabled;
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    const fetchNow = () => {
      backend
        .agentUsage()
        .then((r) => {
          if (live) {
            setReport(r);
            setNowMs(Date.now());
          }
        })
        .catch(() => {});
    };
    fetchNow();
    const t = setInterval(fetchNow, POLL_MS);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [backend, enabled]);
  useEffect(() => setOverrideId(null), [activeId]);

  if (!enabled || !report || overlayOpen) return null;
  const tab = tabs.find((t) => t.id === activeId);
  const picked = pickSession(report, tab);
  if (!picked) return null;
  const shown = (overrideId && report.sessions.find((s) => s.sessionId === overrideId)) || picked.session;
  const account = accountFor(report, shown.kind);
  const { warnAt, dangerAt, anchor, opacity } = settings.agent.widget;
  const nowSec = Math.floor(nowMs / 1000);

  const ctx = shown.context;
  const ctxRatio = ctx ? contextPercent(ctx.usedTokens, ctx.windowTokens) / 100 : 0;
  const ctxLevel = levelOf(ctxRatio, warnAt, dangerAt);
  const staleMin = Math.floor((nowSec - shown.updatedAt) / 60);
  const others = report.sessions.filter((s) => s.sessionId !== shown.sessionId);
  const isOwnTab = shown.tabSessionId && shown.tabSessionId === activeId;
  const runningHere = isOwnTab ? tab?.running : undefined;

  return (
    <section className="agent-widget" data-anchor={anchor} style={{ opacity }} aria-label="Agent 用量">
      <header className="aw-head">
        <strong>
          {KIND_LABEL[shown.kind]}
          {shown.model ? <span className="aw-model"> · {shown.model}</span> : null}
          {account?.plan ? <span className="aw-plan">{account.plan}</span> : null}
        </strong>
        {others.length > 0 ? (
          <button type="button" className="aw-all" onClick={() => setShowAll((v) => !v)}>
            全部 {report.sessions.length} 个 {showAll ? "▴" : "▾"}
          </button>
        ) : (
          <span className="aw-follow">{picked.via === "recent" && !overrideId ? "最近活动" : "跟随当前标签"}</span>
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

      {account && (account.primary || account.secondary) && (
        <>
          <div className="aw-divider" />
          {account.primary && (
            <div className="aw-row">
              <WindowRow label={windowLabel(account.primary.windowMinutes)} w={account.primary} nowSec={nowSec} warnAt={warnAt} dangerAt={dangerAt} />
            </div>
          )}
          {account.secondary && (
            <div className="aw-row">
              <WindowRow label={windowLabel(account.secondary.windowMinutes)} w={account.secondary} nowSec={nowSec} warnAt={warnAt} dangerAt={dangerAt} />
            </div>
          )}
        </>
      )}

      {shown.state !== "unknown" && (
        <div className="aw-state">
          <span className={`aw-dot ${shown.state}`} />
          <span>
            {STATE_TEXT[shown.state]}
            {runningHere ? ` · ${formatElapsed(runningHere.since, nowMs)}` : ""}
          </span>
        </div>
      )}
      {staleMin >= 10 && (
        <div className="aw-stale">
          快照时间 {new Date(shown.updatedAt * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}（本地文件的最近一次记录）
        </div>
      )}

      {showAll && others.length > 0 && (
        <div className="aw-others">
          {others.map((s) => (
            <button
              key={s.sessionId}
              type="button"
              className="aw-other"
              onClick={() => {
                setOverrideId(s.sessionId);
                if (s.tabSessionId) activate(s.tabSessionId);
              }}
            >
              <span>
                {KIND_LABEL[s.kind]}
                {s.model ? ` · ${s.model}` : ""}
              </span>
              <span className="aw-num">{s.context ? `ctx ${Math.round(contextPercent(s.context.usedTokens, s.context.windowTokens))}%` : ""}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
