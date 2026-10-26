// 历史面板（Ctrl+Shift+R）：找回、整理执行过的命令。设计见 docs/04 §3。
// 左边是列表（可按目录、本标签页、收藏、成败、主机筛选），右边是选中命令的详情和每次执行的记录。
//
// 不卡的做法（docs/04 §3"性能"）：搜索在后端的搜索线程里做，界面只发请求、收结果；
// 每次只要一页（100 条），往下滚到快见底时再要下一页；列表是虚拟的，只渲染看得见的几十行；
// 新结果到之前旧结果留在原处，不闪；过时的结果直接丢掉。
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { SUPERSEDED, type Backend, type HistoryDetail, type HistoryHit, type HistoryQuery, type HostCount } from "../backend/types";
import { dayGroup, duration, exitLabel, fullTime, relativeTime, shortDir } from "../history/format";
import { layoutItems, scrollToShow, visibleRange } from "../history/virtual";
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
/** 一页多少条；离列表底部还剩这么多行时要下一页 */
const PAGE = 100;
const PREFETCH_ROWS = 30;
/** 没输入、看全部时，最上面单独列出的收藏 */
const FAV_LIMIT = 30;

interface ListState {
  /** 这份结果对应的查询（变了就作废） */
  key: string;
  hits: HistoryHit[];
  total: number | null;
  hasMore: boolean;
  partial: boolean;
}
const EMPTY_LIST: ListState = { key: "", hits: [], total: 0, hasMore: false, partial: false };

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

/** 面板只关心当前标签页的这几项。只取这些，标签页标题、计时之类的变化就不会让面板重新查询、重画 */
interface TabView {
  id?: string;
  cwd?: string;
  remoteHost?: string;
  remoteConnected: boolean;
  localHost?: string;
  insertable: boolean;
}

const pickTab = (tab: Tab | undefined): TabView => ({
  id: tab?.id,
  cwd: tab?.remote ? tab.remote.cwd : tab?.cwd,
  remoteHost: tab?.remote?.host,
  remoteConnected: !!tab?.remote?.connected,
  localHost: tab?.env?.host,
  insertable: canInsert(tab),
});

const isRemote = (h: { host: string | null }, localHost?: string) =>
  !!h.host && !!localHost && h.host.toLowerCase() !== localHost.toLowerCase();

interface RowProps {
  hit: HistoryHit;
  index: number;
  /** 在虚拟列表里的位置（像素） */
  top: number;
  selected: boolean;
  dedupe: boolean;
  remote: boolean;
  onHover(i: number): void;
  onPick(i: number, run: boolean | null): void;
  onStar(i: number): void;
}

/** 一行。用 memo 包起来：移动选中、输入搜索词时只有变了的行重画 */
const HistoryRow = memo(function HistoryRow({ hit: h, index: i, top, selected, dedupe, remote, onHover, onPick, onStar }: RowProps) {
  return (
    <div className="hx-vitem" style={{ top }}>
      <div
        role="option"
        aria-selected={selected}
        className={`hx-row${selected ? " sel" : ""}`}
        onMouseMove={selected ? undefined : () => onHover(i)}
        onClick={() => onPick(i, null)}
        onDoubleClick={(e) => onPick(i, e.shiftKey)}
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
            onStar(i);
          }}
        >
          <Icon name="star" size={13} />
        </button>
      </div>
    </div>
  );
});

export const HistoryPanel = memo(function HistoryPanel({ backend, onNewTab }: { backend: Backend; onNewTab(cwd?: string): void }) {
  const open = useUi((s) => s.historyOpen);
  const setOpen = useUi((s) => s.setHistory);
  const tab = useStore(useShallow((s) => pickTab(s.tabs.find((t) => t.id === s.activeId))));
  const localHost = tab.localHost;

  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<ScopeKey>("all");
  const [status, setStatus] = useState<Status>("any");
  const [host, setHost] = useState<string>("");
  const [dedupe, setDedupe] = useState(loadPrefs().dedupe);
  const [list, setList] = useState<ListState>(EMPTY_LIST);
  const [favs, setFavs] = useState<HistoryHit[]>([]);
  /** 第一页还在路上（超过一小会儿才显示进度条，快的时候不闪） */
  const [loading, setLoading] = useState(false);
  const [viewport, setViewport] = useState({ top: 0, height: 600 });
  const [hosts, setHosts] = useState<HostCount[]>([]);
  const [sel, setSel] = useState(0);
  const [detail, setDetail] = useState<HistoryDetail | null>(null);
  const [showDetail, setShowDetail] = useState(false);
  const [armedDelete, setArmedDelete] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const listEl = useRef<HTMLDivElement>(null);
  const moreBusy = useRef(false);

  // 打开时：清空输入；连在远程主机上时默认只看那台主机的命令
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setSel(0);
    setArmedDelete(null);
    setHost(tab.remoteConnected && tab.remoteHost ? tab.remoteHost : "");
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

  const { id: tabId, cwd } = tab;
  const baseQuery = useMemo((): HistoryQuery => {
    return {
      text: query,
      scope:
        scope === "cwd" && cwd
          ? { kind: "cwd", value: cwd }
          : scope === "tab" && tabId
            ? { kind: "session", value: tabId }
            : { kind: "all" },
      favoritesOnly: scope === "fav",
      successOnly: status === "ok",
      failedOnly: status === "bad",
      host: host || null,
      dedupe,
      sort: query.trim() ? "relevance" : "recent",
      cwd: cwd ?? null,
      sessionId: tabId ?? null,
      limit: 300,
    };
  }, [query, scope, status, host, dedupe, tabId, cwd]);

  const searching = !!query.trim();
  // 没输入、看全部的时候，把收藏的单独放在最上面
  const pinFavs = !searching && scope === "all";
  const key = `${JSON.stringify(baseQuery)}#${reload}`;
  const keyRef = useRef(key);
  keyRef.current = key;

  // 查第一页：每次按键都直接发（后端同一路只算最新的，排队的旧查询作废），不用防抖；
  // 结果回来之前旧结果留着，回来的若已过时就丢掉
  useEffect(() => {
    if (!open) return;
    let alive = true;
    let retry: number | undefined;
    const slow = window.setTimeout(() => alive && setLoading(true), 150);
    const run = () =>
      backend.historyPage(baseQuery, "panel", 0, PAGE).then(
        (p) => {
          if (!alive) return;
          clearTimeout(slow);
          setLoading(false);
          setList({ key, ...p });
          // 历史库刚启动、还在后台建索引：过一会儿再查一次，把完整的结果补上
          if (p.partial) retry = window.setTimeout(run, 500);
        },
        (e) => {
          if (!alive || String(e).includes(SUPERSEDED)) return;
          clearTimeout(slow);
          setLoading(false);
          setList({ ...EMPTY_LIST, key });
        },
      );
    run();
    if (pinFavs) {
      backend
        .historyPage({ ...baseQuery, favoritesOnly: true, dedupe: true }, "panel-fav", 0, FAV_LIMIT)
        .then((p) => alive && setFavs(p.hits), () => {});
    } else setFavs([]);
    return () => {
      alive = false;
      clearTimeout(slow);
      clearTimeout(retry);
    };
    // key 已经包含了 baseQuery 和 reload
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, key, backend]);

  // 收藏已经列在最上面了，下面的结果里不再重复
  const rows = useMemo(() => {
    if (!pinFavs || favs.length === 0) return list.hits;
    const favTexts = new Set(favs.map((x) => x.text));
    return [...favs, ...list.hits.filter((x) => !favTexts.has(x.text))];
  }, [favs, list.hits, pinFavs]);
  const favCount = pinFavs ? favs.length : 0;

  // 要下一页：同一路，后端从缓存的排序结果里直接切，几乎不花时间
  const loadMore = useCallback(() => {
    const l = live.current.list;
    if (!l.hasMore || moreBusy.current || l.key !== keyRef.current) return;
    moreBusy.current = true;
    backend
      .historyPage(live.current.baseQuery, "panel", l.hits.length, PAGE)
      .then(
        (p) =>
          setList((cur) =>
            cur.key === l.key && cur.hits.length === l.hits.length
              ? { ...cur, hits: [...cur.hits, ...p.hits], hasMore: p.hasMore, total: p.total ?? cur.total, partial: p.partial }
              : cur,
          ),
        () => {},
      )
      .finally(() => (moreBusy.current = false));
  }, [backend]);

  // 虚拟列表的排版：分组标题 + 每行的位置
  const layout = useMemo(
    () => layoutItems(rows.length, (i) => (i < favCount ? "收藏" : searching ? "" : dayGroup(rows[i].lastUsed))),
    [rows, favCount, searching],
  );
  const [start, end] = visibleRange(layout.items, viewport.top, viewport.height);
  const lastShown = end > 0 ? layout.items[end - 1] : undefined;
  const lastShownRow = lastShown?.kind === "row" ? lastShown.index : -1;
  useEffect(() => {
    if (lastShownRow >= rows.length - PREFETCH_ROWS) loadMore();
  }, [lastShownRow, rows.length, loadMore]);

  const onScroll = useCallback(() => {
    const el = listEl.current;
    if (el) setViewport({ top: el.scrollTop, height: el.clientHeight });
  }, []);
  useEffect(() => {
    const el = listEl.current;
    if (!open || !el) return;
    const ro = new ResizeObserver(onScroll);
    ro.observe(el);
    onScroll();
    return () => ro.disconnect();
  }, [open, onScroll]);

  // 换了查询：回到顶部
  useEffect(() => {
    setSel(0);
    if (listEl.current) listEl.current.scrollTop = 0;
  }, [baseQuery]);
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

  // 键盘移动选中时，让选中的行露出来
  useEffect(() => {
    const el = listEl.current;
    const top = layout.rowTop[sel];
    if (!el || top === undefined) return;
    const next = scrollToShow(el.scrollTop, el.clientHeight, top);
    if (next !== el.scrollTop) el.scrollTop = next;
    // 只在选中变化时滚
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel]);

  const close = useCallback(() => setOpen(false), [setOpen]);
  const insertable = tab.insertable;

  const send = (h: HistoryHit | undefined, run: boolean) => {
    if (!h || !tabId || !insertable) return;
    close();
    backend.write(tabId, h.text + (run ? "\r" : ""));
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

  const isRemoteHit = (h: HistoryHit) => isRemote(h, localHost);

  // 给每一行的回调要保持不变（memo 才有用），所以通过 ref 拿最新的数据和函数
  const live = useRef({ rows, send, toggleFav, list, baseQuery });
  live.current = { rows, send, toggleFav, list, baseQuery };
  const onHover = useCallback((i: number) => setSel(i), []);
  const onPick = useCallback((i: number, run: boolean | null) => {
    setSel(i);
    if (run !== null) live.current.send(live.current.rows[i], run);
  }, []);
  const onStar = useCallback((i: number) => {
    live.current.toggleFav(live.current.rows[i]);
  }, []);

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

  const remoteHosts = hosts.filter((h) => h.remote);
  const cwdLabel = shortDir(cwd);

  return (
    <div className="pal-scrim hx-scrim" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className={`history-panel${showDetail ? " show-detail" : ""}`} role="dialog" aria-label="历史命令" onKeyDown={onKey}>
        <div className="pal-input">
          <Icon name="history" />
          <input
            ref={input}
            value={query}
            placeholder={tab.remoteConnected ? `搜索历史命令（默认只看 ${tab.remoteHost}）` : "搜索执行过的命令"}
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
          <div className="hx-list" ref={listEl} role="listbox" aria-label="命令" onScroll={onScroll}>
            {loading && <div className="hx-loading" aria-hidden />}
            {rows.length === 0 && !loading && list.key === key && (
              <div className="pal-empty">
                {searching ? "没有找到匹配的命令" : scope === "fav" ? "还没有收藏的命令。选中一条按 Ctrl+S 收藏" : "这里还没有记录"}
              </div>
            )}
            <div className="hx-space" style={{ height: layout.height }}>
              {layout.items.slice(start, end).map((it) =>
                it.kind === "group" ? (
                  <div key={`g-${it.top}`} className="pal-group hx-vitem hx-vgroup" style={{ top: it.top }}>
                    {it.label}
                  </div>
                ) : (
                  <HistoryRow
                    key={`${it.index < favCount ? "f" : "h"}-${rows[it.index].id}`}
                    hit={rows[it.index]}
                    index={it.index}
                    top={it.top}
                    selected={it.index === sel}
                    dedupe={dedupe}
                    remote={isRemoteHit(rows[it.index])}
                    onHover={onHover}
                    onPick={onPick}
                    onStar={onStar}
                  />
                ),
              )}
            </div>
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
          <span className="hx-count-foot">
            {list.partial ? "正在载入完整历史…" : list.total !== null ? `${list.total} 条` : list.hasMore ? `${list.hits.length}+ 条` : `${list.hits.length} 条`}
          </span>
        </div>
      </div>
    </div>
  );
});
