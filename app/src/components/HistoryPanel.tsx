// 历史面板（Ctrl+Shift+R）：找回、整理执行过的命令。设计见 docs/04 §3。
// 左边是列表（可按目录、本标签页、收藏、成败、主机筛选），右边是选中命令的详情和每次执行的记录。
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Backend, HistoryDetail, HistoryHit, HistoryQuery, HostCount } from "../backend/types";
import { dayGroup, duration, exitLabel, fullTime, relativeTime, shortDir } from "../history/format";
import { useStore, type Tab } from "../state/store";
import { useUi } from "../state/ui";
import { Icon } from "./Icons";

type ScopeKey = "all" | "cwd" | "tab" | "fav";
type Status = "any" | "ok" | "bad";

const SCOPES: { key: ScopeKey; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "cwd", label: "当前目录" },
  { key: "tab", label: "本标签页" },
  { key: "fav", label: "收藏" },
];

const PREFS_KEY = "mooncli.history";

function loadPrefs(): { dedupe: boolean } {
  try {
    return { dedupe: true, ...JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") };
  } catch {
    return { dedupe: true };
  }
}

/** 当前标签页能不能接收一条命令：本机 pwsh 在等输入，或者远程 shell 在等输入 */
export function canInsert(tab: Tab | undefined): boolean {
  if (!tab || tab.exited) return false;
  if (tab.remote) return tab.remote.connected && !tab.running && tab.shellState !== "altScreen";
  return tab.shellState === "prompt" || tab.shellState === "unknown";
}

function Highlight({ text, indices }: { text: string; indices: number[] }) {
  if (indices.length === 0) return <>{text}</>;
  const set = new Set(indices);
  return (
    <>
      {[...text].map((c, i) => (set.has(i) ? <b key={i}>{c}</b> : <span key={i}>{c}</span>))}
    </>
  );
}

function ExitMark({ code }: { code: number | null }) {
  const e = exitLabel(code);
  if (e.kind === "ok") return <span className="hx-exit ok" title={e.text}>✓</span>;
  if (e.kind === "bad") return <span className="hx-exit bad" title={e.text}>✗ {code}</span>;
  if (e.kind === "stop") return <span className="hx-exit stop" title={e.text}>⏹</span>;
  return <span className="hx-exit none" title={e.text}>·</span>;
}

export function HistoryPanel({ backend, onNewTab }: { backend: Backend; onNewTab(cwd?: string): void }) {
  const open = useUi((s) => s.historyOpen);
  const setOpen = useUi((s) => s.setHistory);
  const tab = useStore((s) => s.tabs.find((t) => t.id === s.activeId));
  const localHost = tab?.env?.host;

  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<ScopeKey>("all");
  const [status, setStatus] = useState<Status>("any");
  const [host, setHost] = useState<string>("");
  const [dedupe, setDedupe] = useState(loadPrefs().dedupe);
  const [hits, setHits] = useState<HistoryHit[]>([]);
  const [favs, setFavs] = useState<HistoryHit[]>([]);
  const [hosts, setHosts] = useState<HostCount[]>([]);
  const [sel, setSel] = useState(0);
  const [detail, setDetail] = useState<HistoryDetail | null>(null);
  const [showDetail, setShowDetail] = useState(false);
  const [armedDelete, setArmedDelete] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  // 打开时：清空输入；连在远程主机上时默认只看那台主机的命令
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setSel(0);
    setArmedDelete(null);
    setHost(tab?.remote?.connected ? tab.remote.host : "");
    backend.historyHosts().then(setHosts, () => setHosts([]));
    requestAnimationFrame(() => input.current?.focus());
    // 只在打开时取一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify({ dedupe }));
    } catch {
      // 存不了就算了
    }
  }, [dedupe]);

  const baseQuery = useMemo((): HistoryQuery => {
    const cwd = tab?.remote ? tab.remote.cwd : tab?.cwd;
    return {
      text: query,
      scope:
        scope === "cwd" && cwd
          ? { kind: "cwd", value: cwd }
          : scope === "tab" && tab
            ? { kind: "session", value: tab.id }
            : { kind: "all" },
      favoritesOnly: scope === "fav",
      successOnly: status === "ok",
      failedOnly: status === "bad",
      host: host || null,
      dedupe,
      sort: query.trim() ? "relevance" : "recent",
      cwd: cwd ?? null,
      sessionId: tab?.id ?? null,
      limit: 300,
    };
  }, [query, scope, status, host, dedupe, tab]);

  // 输入停顿 60ms 再查；没输入、看全部的时候，把收藏的单独放在最上面
  useEffect(() => {
    if (!open) return;
    let alive = true;
    const t = setTimeout(() => {
      const pinFavs = !query.trim() && scope === "all";
      Promise.all([
        backend.historyQuery(baseQuery),
        pinFavs ? backend.historyQuery({ ...baseQuery, favoritesOnly: true, dedupe: true, limit: 30 }) : Promise.resolve([]),
      ]).then(
        ([h, f]) => {
          if (!alive) return;
          const favTexts = new Set(f.map((x) => x.text));
          setFavs(f);
          setHits(pinFavs ? h.filter((x) => !favTexts.has(x.text)) : h);
        },
        () => alive && (setHits([]), setFavs([])),
      );
    }, 60);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [open, baseQuery, backend, query, scope, reload]);

  const rows = useMemo(() => [...favs, ...hits], [favs, hits]);
  useEffect(() => setSel(0), [baseQuery]);
  const current = rows[Math.min(sel, rows.length - 1)];

  // 选中的命令的详情
  useEffect(() => {
    if (!open || !current) {
      setDetail(null);
      return;
    }
    let alive = true;
    const t = setTimeout(() => backend.historyDetail(current.text).then((d) => alive && setDetail(d), () => alive && setDetail(null)), 80);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [open, current?.text, backend, reload]);

  useEffect(() => {
    list.current?.querySelector(".hx-row.sel")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const close = useCallback(() => setOpen(false), [setOpen]);
  const insertable = canInsert(tab);

  const send = (h: HistoryHit | undefined, run: boolean) => {
    if (!h || !tab || !insertable) return;
    close();
    backend.write(tab.id, h.text + (run ? "\r" : ""));
  };

  const copy = (h: HistoryHit | undefined) => {
    if (!h) return;
    backend.clipboardWrite(h.text).catch(() => {});
    useUi.getState().showNotice("已复制命令");
  };

  const toggleFav = async (h: HistoryHit | undefined) => {
    if (!h) return;
    await backend.historyFavorite(h.text, !h.favorite).catch(() => {});
    setReload((r) => r + 1);
  };

  const remove = async (h: HistoryHit | undefined) => {
    if (!h) return;
    if (armedDelete !== h.text) {
      setArmedDelete(h.text);
      return;
    }
    setArmedDelete(null);
    const n = await backend.historyDelete(h.text).catch(() => 0);
    useUi.getState().showNotice(`已删除 ${n} 条记录`);
    setReload((r) => r + 1);
  };

  const openHere = (h: HistoryHit | undefined) => {
    if (!h?.lastCwd || isRemoteHit(h)) return;
    close();
    onNewTab(h.lastCwd);
  };

  const isRemoteHit = (h: HistoryHit) => !!h.host && !!localHost && h.host.toLowerCase() !== localHost.toLowerCase();

  if (!open) return null;

  const onKey = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    const k = e.key;
    if (k !== "Delete") setArmedDelete(null);
    if (k === "Escape") {
      if (showDetail) setShowDetail(false);
      else close();
    } else if (k === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(rows.length - 1, s + 1));
    } else if (k === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(0, s - 1));
    } else if (k === "PageDown") {
      e.preventDefault();
      setSel((s) => Math.min(rows.length - 1, s + 10));
    } else if (k === "PageUp") {
      e.preventDefault();
      setSel((s) => Math.max(0, s - 10));
    } else if (k === "Enter") {
      e.preventDefault();
      send(current, e.shiftKey);
    } else if (k === "Tab") {
      e.preventDefault();
      setShowDetail((v) => !v);
    } else if (e.altKey && (k === "d" || k === "D")) {
      e.preventDefault();
      setDedupe((v) => !v);
    } else if (e.altKey && /^[1-4]$/.test(k)) {
      e.preventDefault();
      setScope(SCOPES[Number(k) - 1].key);
    } else if (e.ctrlKey && (k === "c" || k === "C") && !window.getSelection()?.toString()) {
      e.preventDefault();
      copy(current);
    } else if (e.ctrlKey && (k === "s" || k === "S")) {
      e.preventDefault();
      toggleFav(current);
    } else if (e.ctrlKey && (k === "o" || k === "O")) {
      e.preventDefault();
      openHere(current);
    } else if (k === "Delete" && e.shiftKey) {
      e.preventDefault();
      remove(current);
    }
  };

  const searching = !!query.trim();
  let lastGroup = "";
  const groupOf = (h: HistoryHit, i: number) => (i < favs.length ? "收藏" : searching ? "" : dayGroup(h.lastUsed));
  const remoteHosts = hosts.filter((h) => h.remote);
  const cwdLabel = shortDir(tab?.remote ? tab.remote.cwd : tab?.cwd);

  return (
    <div className="pal-scrim hx-scrim" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className={`history-panel${showDetail ? " show-detail" : ""}`} role="dialog" aria-label="历史命令" onKeyDown={onKey}>
        <div className="pal-input">
          <Icon name="history" />
          <input
            ref={input}
            value={query}
            placeholder={tab?.remote?.connected ? `搜索历史命令（默认只看 ${tab.remote.host}）` : "搜索执行过的命令"}
            aria-label="搜索历史命令"
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            type="button"
            className={`hx-chip${dedupe ? " on" : ""}`}
            aria-pressed={dedupe}
            title="相同的命令只显示一条 (Alt+D)"
            onClick={() => setDedupe((v) => !v)}
          >
            合并重复
          </button>
          <button type="button" className="icon-btn" aria-label="关闭 (Esc)" title="关闭 (Esc)" onClick={close}>
            <Icon name="close" />
          </button>
        </div>

        <div className="hx-filters">
          <div className="hx-seg" role="radiogroup" aria-label="范围">
            {SCOPES.map((s, i) => (
              <button
                key={s.key}
                type="button"
                role="radio"
                aria-checked={scope === s.key}
                className={scope === s.key ? "on" : ""}
                title={`${s.key === "cwd" && cwdLabel ? cwdLabel : s.label} (Alt+${i + 1})`}
                disabled={s.key === "cwd" && !cwdLabel}
                onClick={() => setScope(s.key)}
              >
                {s.key === "fav" && <Icon name="star" size={12} />}
                {s.label}
              </button>
            ))}
          </div>
          <div className="hx-seg" role="radiogroup" aria-label="结果">
            {(
              [
                ["any", "不限结果"],
                ["ok", "成功"],
                ["bad", "失败"],
              ] as const
            ).map(([k, l]) => (
              <button key={k} type="button" role="radio" aria-checked={status === k} className={status === k ? "on" : ""} onClick={() => setStatus(k)}>
                {l}
              </button>
            ))}
          </div>
          {hosts.length > 1 && (
            <label className="hx-host">
              <Icon name="ssh" size={13} />
              <select value={host} aria-label="主机" onChange={(e) => setHost(e.target.value)}>
                <option value="">所有主机</option>
                {hosts
                  .filter((h) => !h.remote)
                  .map((h) => (
                    <option key={h.host} value={h.host}>
                      本机 {h.host}
                    </option>
                  ))}
                {remoteHosts.length > 0 && (
                  <optgroup label="远程">
                    {remoteHosts.map((h) => (
                      <option key={h.host} value={h.host}>
                        {h.host}（{h.count}）
                      </option>
                    ))}
                  </optgroup>
                )}
              </select>
            </label>
          )}
        </div>

        <div className="hx-body">
          <div className="hx-list" ref={list} role="listbox" aria-label="命令">
            {rows.length === 0 && (
              <div className="pal-empty">
                {searching ? "没有找到匹配的命令" : scope === "fav" ? "还没有收藏的命令。选中一条按 Ctrl+S 收藏" : "这里还没有记录"}
              </div>
            )}
            {rows.map((h, i) => {
              const g = groupOf(h, i);
              const head = g && g !== lastGroup ? g : null;
              lastGroup = g;
              const remote = isRemoteHit(h);
              return (
                <div key={`${i < favs.length ? "f" : "h"}-${h.id}`}>
                  {head && <div className="pal-group">{head}</div>}
                  <div
                    role="option"
                    aria-selected={i === sel}
                    className={`hx-row${i === sel ? " sel" : ""}`}
                    onMouseMove={() => i !== sel && setSel(i)}
                    onClick={() => setSel(i)}
                    onDoubleClick={(e) => send(h, e.shiftKey)}
                  >
                    <ExitMark code={h.lastExit} />
                    <span className="hx-text">
                      <Highlight text={h.text} indices={h.matched} />
                    </span>
                    <span className="hx-where">
                      {remote && <span className="hx-host-tag">{h.host}</span>}
                      {shortDir(h.lastCwd)}
                    </span>
                    {dedupe && <span className="hx-count">{h.count > 1 ? `${h.count} 次` : ""}</span>}
                    <span className="hx-time">{relativeTime(h.lastUsed)}</span>
                    <button
                      type="button"
                      className={`hx-star${h.favorite ? " on" : ""}`}
                      aria-label={h.favorite ? "取消收藏" : "收藏"}
                      title={h.favorite ? "取消收藏 (Ctrl+S)" : "收藏 (Ctrl+S)"}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFav(h);
                      }}
                    >
                      <Icon name="star" size={13} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>

          <aside className="hx-detail" aria-label="详情">
            {current ? (
              <>
                <button type="button" className="hx-back" onClick={() => setShowDetail(false)}>
                  ← 返回列表 <kbd>Tab</kbd>
                </button>
                <pre className="hx-cmd">{current.text}</pre>
                <div className="hx-actions">
                  <button type="button" className="btn primary" disabled={!insertable} title={insertable ? "填入输入行，可以再改 (Enter)" : "当前标签页不在提示符上"} onClick={() => send(current, false)}>
                    填入 <kbd>Enter</kbd>
                  </button>
                  <button type="button" className="btn" disabled={!insertable} title="直接运行 (Shift+Enter)" onClick={() => send(current, true)}>
                    运行
                  </button>
                  <button type="button" className="btn" title="复制 (Ctrl+C)" onClick={() => copy(current)}>
                    复制
                  </button>
                  {current.lastCwd && !isRemoteHit(current) && (
                    <button type="button" className="btn" title="在这条命令的目录打开新标签页 (Ctrl+O)" onClick={() => openHere(current)}>
                      在此目录打开
                    </button>
                  )}
                  <button type="button" className={`btn${armedDelete === current.text ? " danger" : ""}`} title="删除这条命令的全部记录 (Shift+Del)" onClick={() => remove(current)}>
                    {armedDelete === current.text ? "再点一次删除" : "删除"}
                  </button>
                </div>
                {detail && (
                  <>
                    <dl className="hx-stats">
                      <div>
                        <dt>执行</dt>
                        <dd>{detail.summary.count} 次</dd>
                      </div>
                      <div>
                        <dt>成功 / 失败</dt>
                        <dd>
                          <span className="ok">{detail.summary.ok}</span> / <span className={detail.summary.failed ? "bad" : ""}>{detail.summary.failed}</span>
                        </dd>
                      </div>
                      <div>
                        <dt>平均耗时</dt>
                        <dd>{duration(detail.summary.avgDurationMs) || "—"}</dd>
                      </div>
                      <div>
                        <dt>第一次用</dt>
                        <dd>{detail.summary.firstUsed ? relativeTime(detail.summary.firstUsed) : "—"}</dd>
                      </div>
                    </dl>
                    <div className="hx-runs-title">最近的执行</div>
                    <ul className="hx-runs">
                      {detail.runs.map((r) => (
                        <li key={r.id}>
                          <ExitMark code={r.exitCode} />
                          <span className="hx-run-time">{fullTime(r.startedAt)}</span>
                          <span className="hx-run-where" title={r.cwd ?? undefined}>
                            {r.host && localHost && r.host.toLowerCase() !== localHost.toLowerCase() && <span className="hx-host-tag">{r.host}</span>}
                            {r.imported ? "从 PSReadLine 导入" : shortDir(r.cwd)}
                          </span>
                          <span className="hx-run-dur">{duration(r.durationMs)}</span>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
              </>
            ) : (
              <div className="pal-empty">选中一条命令查看详情</div>
            )}
          </aside>
        </div>

        <div className="pal-foot">
          <span>
            <kbd>Enter</kbd> 填入
          </span>
          <span>
            <kbd>Shift+Enter</kbd> 运行
          </span>
          <span>
            <kbd>Ctrl+C</kbd> 复制
          </span>
          <span>
            <kbd>Ctrl+S</kbd> 收藏
          </span>
          <span>
            <kbd>Shift+Del</kbd> 删除
          </span>
          <span className="hx-foot-tab">
            <kbd>Tab</kbd> 详情
          </span>
          <span>
            <kbd>Alt+1~4</kbd> 范围
          </span>
        </div>
      </div>
    </div>
  );
}
