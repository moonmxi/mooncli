// 补全面板（docs/04 §4）：Ctrl+Space 在光标处弹出；Ctrl+P 钉在右侧常驻、随输入刷新。
// 候选来自 pwsh 自己的补全（悄悄发 F12,F11，集成脚本用私有序列送回）、命令说明书、历史库。
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Backend } from "../backend/types";
import { keystrokes, mergeCandidates, parsePwsh, type Candidate, type PwshCompletions } from "../completion/merge";
import { useSettings } from "../state/settings";
import { focusedTab, useStore } from "../state/store";
import { useUi } from "../state/ui";
import { cursorPoint, getTerminal, onCompletions, readInput } from "../terminal/TerminalView";

/** PSReadLine 里注册的取补全组合键：F12 F11（shell/pwsh/MoonCli.ps1） */
const REQUEST_KEYS = "\x1b[24~\x1b[23~";
const PANEL_W = 420;

const KIND_CLASS: Record<string, string> = { 历史: "hist", 参数: "opt", 子命令: "sub", 命令: "cmd", 文件夹: "dir", 文件: "file" };

/** docked：钉住时占右侧一栏（宽窗口）；否则钉住的面板浮在终端右侧 */
export function CompletionPanel({ backend, docked, uiScale }: { backend: Backend; docked: boolean; uiScale: number }) {
  const comp = useUi((s) => s.completion);
  const close = useUi((s) => s.closeCompletion);
  const pin = useUi((s) => s.pinCompletion);
  const pwshInPinned = useSettings((s) => s.settings.completion.pinnedRequestsPwsh);
  const tab = useStore((s) => s.tabs.find((t) => t.id === comp?.id));
  const isCurrent = useStore((s) => focusedTab(s)?.id === comp?.id);
  const [input, setInput] = useState<{ line: string; cursor: number } | null>(null);
  const [pwsh, setPwsh] = useState<PwshCompletions | null>(null);
  const [history, setHistory] = useState<{ text: string; count: number }[]>([]);
  const [sel, setSel] = useState(0);
  const [waiting, setWaiting] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const id = comp?.id;
  const pinned = !!comp?.pinned;
  const atPrompt = tab?.shellState === "prompt" && !tab.remote;

  // 读输入行 + 查历史（防抖 80ms）
  const refresh = useCallback(() => {
    if (!id) return;
    const inp = readInput(id);
    setInput(inp);
    if (!inp || !inp.line.trim()) {
      setHistory([]);
      return;
    }
    const cwd = useStore.getState().tabs.find((t) => t.id === id)?.cwd;
    // 只要以当前输入开头的历史：补全面板里模糊匹配的结果大多是噪音（模糊搜索在历史面板里有）
    const typed = inp.line.trim();
    const lower = typed.toLowerCase();
    backend
      .historySearch(typed, { cwd, limit: 40 })
      .then((hits) => setHistory(hits.filter((h) => h.text.toLowerCase().startsWith(lower)).slice(0, 8).map((h) => ({ text: h.text, count: h.count }))))
      .catch(() => setHistory([]));
  }, [backend, id]);

  // 打开 / 再按一次 Ctrl+Space：读输入、向 pwsh 要补全
  useEffect(() => {
    if (!id) return;
    setSel(0);
    setPwsh(null);
    refresh();
    const t = useStore.getState().tabs.find((x) => x.id === id);
    if (t?.shellState === "prompt" && !t.remote) {
      setWaiting(true);
      backend.write(id, REQUEST_KEYS);
    }
    const timer = setTimeout(() => setWaiting(false), 1500);
    return () => clearTimeout(timer);
  }, [id, comp?.seq, backend, refresh]);

  useEffect(
    () =>
      onCompletions((from, data) => {
        if (from !== id) return;
        const p = parsePwsh(data);
        setPwsh(p);
        setWaiting(false);
        if (p) setInput({ line: p.line, cursor: p.cursor });
      }),
    [id],
  );

  // 钉住时随输入刷新：光标一动就重读（防抖），pwsh 补全默认不请求（悄悄发按键可能和快速输入抢顺序）
  useEffect(() => {
    if (!id || !pinned) return;
    const term = getTerminal(id)?.term;
    if (!term) return;
    let t1 = 0;
    let t2 = 0;
    const sub = term.onCursorMove(() => {
      clearTimeout(t1);
      t1 = window.setTimeout(refresh, 80);
      if (pwshInPinned) {
        clearTimeout(t2);
        t2 = window.setTimeout(() => {
          const st = useStore.getState().tabs.find((x) => x.id === id);
          if (st?.shellState === "prompt" && !st.remote) backend.write(id, REQUEST_KEYS);
        }, 400);
      }
    });
    return () => {
      sub.dispose();
      clearTimeout(t1);
      clearTimeout(t2);
    };
  }, [id, pinned, pwshInPinned, refresh, backend]);

  // 没钉住时拿走键盘焦点，方向键、回车给面板
  useEffect(() => {
    if (id && !pinned) requestAnimationFrame(() => ref.current?.focus());
  }, [id, pinned, comp?.seq]);

  const list: Candidate[] = useMemo(
    () => (input ? mergeCandidates({ line: input.line, cursor: input.cursor, pwsh, history }) : []),
    [input, pwsh, history],
  );
  useEffect(() => setSel((s) => Math.min(s, Math.max(0, list.length - 1))), [list.length]);

  const backToTerminal = useCallback(() => {
    if (id) getTerminal(id)?.term.focus();
  }, [id]);

  const apply = useCallback(
    (c: Candidate, keepOpen: boolean) => {
      if (!id || !input) return;
      backend.write(id, keystrokes(input.line, input.cursor, c));
      if (keepOpen) {
        // → 接受并继续：等 pwsh 把字画出来再要下一段的补全
        setTimeout(() => useUi.getState().openCompletion(id), 60);
      } else if (!pinned) {
        close();
      }
      backToTerminal();
    },
    [id, input, backend, pinned, close, backToTerminal],
  );

  // 钉住时 Alt+↑↓ 选、Alt+Enter 填入（不离开输入行）
  useEffect(() => {
    if (!pinned) return;
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.ctrlKey || !atPrompt || !isCurrent) return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        setSel((s) => (list.length ? (s + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length : 0));
      } else if (e.key === "Enter" && list[sel]) {
        e.preventDefault();
        e.stopPropagation();
        apply(list[sel], false);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [pinned, atPrompt, isCurrent, list, sel, apply]);

  // 命令开始执行、切走标签页：没钉住的面板直接关掉
  useEffect(() => {
    if (comp && !pinned && (!atPrompt || !isCurrent)) close();
  }, [comp, pinned, atPrompt, isCurrent, close]);

  useEffect(() => {
    ref.current?.querySelector(".cp-item.sel")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  if (!comp || !id) return null;
  if (pinned && !isCurrent) return null;

  const onKeyDown = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    const k = e.key;
    if (k === "ArrowDown" || k === "ArrowUp") {
      e.preventDefault();
      if (list.length) setSel((s) => (s + (k === "ArrowDown" ? 1 : -1) + list.length) % list.length);
    } else if ((k === "Enter" || k === "Tab") && list[sel]) {
      e.preventDefault();
      apply(list[sel], false);
    } else if (k === "ArrowRight" && list[sel]) {
      e.preventDefault();
      apply(list[sel], true);
    } else if (k === "Escape") {
      e.preventDefault();
      close();
      backToTerminal();
    } else if (e.ctrlKey && (k === "p" || k === "P")) {
      e.preventDefault();
      pin(!pinned);
      backToTerminal();
    } else if (!pinned && !e.ctrlKey && !e.altKey && (k.length === 1 || k === "Backspace")) {
      // 继续打字：关掉面板，这个键照常交给 shell
      e.preventDefault();
      close();
      backend.write(id, k === "Backspace" ? "\x7f" : k);
      backToTerminal();
    }
  };

  let style: React.CSSProperties = {};
  if (!pinned) {
    const p = cursorPoint(id);
    // 面板带着界面缩放（CSS zoom），定位坐标也要按缩放换算
    const z = uiScale || 1;
    const listH = Math.min(320, 34 + list.length * 30) * z;
    const w = PANEL_W * z;
    if (p) {
      const below = p.y + listH + 8 < innerHeight - 30;
      style = {
        left: Math.max(8, Math.min(p.x - 12, innerWidth - w - 8)) / z,
        top: (below ? p.y + 4 : Math.max(8, p.y - p.lineHeight - listH - 4)) / z,
      };
    }
  }

  const empty = waiting ? "正在向 pwsh 要补全…" : !atPrompt ? "当前程序不在 pwsh 提示符上" : input && !input.line.trim() ? "输入命令后这里会出现候选" : "没有候选";

  return (
    <div
      ref={ref}
      className={`completion${pinned ? " pinned" : ""}${pinned && docked ? " docked" : ""}${pinned && !atPrompt ? " idle" : ""}`}
      style={style}
      tabIndex={-1}
      role="listbox"
      aria-label="补全"
      onKeyDown={onKeyDown}
      onBlur={(e) => {
        // 读最新状态：刚按 Ctrl+P 钉住时焦点会回到终端，这时不能关
        if (!useUi.getState().completion?.pinned && !e.currentTarget.contains(e.relatedTarget as Node)) close();
      }}
    >
      {pinned && (
        <div className="cp-head">
          <strong>补全</strong>
          <span className="cp-input">{input?.line.slice(0, input.cursor) || " "}</span>
          <button type="button" className="cp-btn" title="取消钉住 (Ctrl+P)" onClick={() => pin(false)}>
            取消钉住
          </button>
          <button type="button" className="cp-btn" title="关闭" onClick={() => close()}>
            ×
          </button>
        </div>
      )}
      <div className="cp-list">
        {list.length === 0 ? (
          <div className="cp-empty">{empty}</div>
        ) : (
          list.map((c, i) => (
            <div
              key={c.key}
              role="option"
              aria-selected={i === sel}
              className={`cp-item${i === sel ? " sel" : ""}`}
              onMouseEnter={() => !pinned && setSel(i)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => apply(c, false)}
            >
              <span className={`cp-kind k-${KIND_CLASS[c.kind] ?? "other"}`}>{c.kind}</span>
              <span className="cp-label">{c.label}</span>
              {c.desc && <span className="cp-desc">{c.desc}</span>}
            </div>
          ))
        )}
      </div>
      <div className="cp-foot">{pinned ? "Alt+↑↓ 选择 · Alt+Enter 填入 · 点击填入" : "↑↓ 选择 · Enter 填入 · → 接受并继续 · Esc 关闭 · Ctrl+P 钉住"}</div>
    </div>
  );
}
