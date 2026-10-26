// 命令面板（Ctrl+Shift+P，或点标题栏的搜索框）：搜索操作、主题、标签页和历史命令。
// 历史命令：Enter 填入当前标签页，Shift+Enter 直接执行（docs/04 §3 的同一套约定）。
import { useEffect, useMemo, useRef, useState } from "react";
import type { Backend, HistoryHit } from "../backend/types";
import { bestMatch } from "../palette/match";
import { useStore } from "../state/store";
import { useUi } from "../state/ui";
import { Icon } from "./Icons";

export interface PaletteCommand {
  id: string;
  label: string;
  group: string;
  icon?: string;
  hint?: string;
  keywords?: string[];
  run(): void;
}

interface Row {
  key: string;
  label: string;
  group: string;
  icon: string;
  hint?: string;
  indices: number[];
  score: number;
  run(shift: boolean): void;
  disabled?: string;
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

export function CommandPalette({ commands, backend }: { commands: PaletteCommand[]; backend: Backend }) {
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPalette);
  const activeTab = useStore((s) => s.tabs.find((t) => t.id === s.activeId));
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const [history, setHistory] = useState<HistoryHit[]>([]);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSel(0);
      requestAnimationFrame(() => input.current?.focus());
    }
  }, [open]);

  // 历史命令从后端查（Rust 端模糊搜索），输入停顿 60ms 再查
  useEffect(() => {
    if (!open || !query.trim()) {
      setHistory([]);
      return;
    }
    let alive = true;
    const t = setTimeout(() => {
      backend
        .historySearch(query, { cwd: activeTab?.cwd, limit: 8, lane: "palette" })
        .then((h) => alive && setHistory(h))
        .catch(() => alive && setHistory([]));
    }, 60);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [open, query, backend, activeTab?.cwd]);

  const rows: Row[] = useMemo(() => {
    const out: Row[] = [];
    for (const c of commands) {
      const m = bestMatch(c.label, c.keywords ?? [], query);
      if (!m) continue;
      out.push({ key: c.id, label: c.label, group: c.group, icon: c.icon ?? "other", hint: c.hint, indices: m.indices, score: m.score, run: () => c.run() });
    }
    out.sort((a, b) => b.score - a.score);
    // 历史命令只能填进正在等待输入的 pwsh；程序运行中填进去会变成那个程序的输入
    const canInsert = !activeTab || activeTab.shellState === "prompt" || activeTab.shellState === "unknown";
    const hist: Row[] = history.map((h) => ({
      key: `h-${h.id}`,
      label: h.text,
      group: "历史命令",
      icon: "history",
      hint: h.count > 1 ? `${h.count} 次` : undefined,
      indices: h.matched,
      score: 0,
      disabled: canInsert ? undefined : "当前程序不在 pwsh 提示符上",
      run: (shift) => {
        if (!activeTab) return;
        backend.write(activeTab.id, h.text + (shift ? "\r" : ""));
      },
    }));
    // 有输入时：最匹配的操作在前，再接历史命令；没有输入时只列操作
    return query.trim() ? [...out.slice(0, 12), ...hist, ...out.slice(12)] : out;
  }, [commands, query, history, activeTab, backend]);

  useEffect(() => setSel(0), [query]);
  useEffect(() => {
    list.current?.querySelector(".pal-row.sel")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  if (!open) return null;

  const choose = (r: Row | undefined, shift: boolean) => {
    if (!r || r.disabled) return;
    setOpen(false);
    r.run(shift);
  };

  let lastGroup = "";
  return (
    <div className="pal-scrim" onMouseDown={(e) => e.target === e.currentTarget && setOpen(false)}>
      <div className="palette" role="dialog" aria-label="命令面板">
        <div className="pal-input">
          <Icon name="search" />
          <input
            ref={input}
            value={query}
            placeholder="搜索操作、主题、标签页或历史命令"
            aria-label="搜索"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Escape") setOpen(false);
              else if (e.key === "ArrowDown") {
                e.preventDefault();
                setSel((s) => Math.min(rows.length - 1, s + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSel((s) => Math.max(0, s - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                choose(rows[sel], e.shiftKey);
              }
            }}
          />
          <kbd>Esc</kbd>
        </div>
        <div className="pal-list" ref={list} role="listbox">
          {rows.length === 0 && <div className="pal-empty">没有找到匹配的操作或命令</div>}
          {rows.map((r, i) => {
            const searching = !!query.trim();
            const head = !searching && r.group !== lastGroup ? r.group : null;
            lastGroup = r.group;
            return (
              <div key={r.key}>
                {head && <div className="pal-group">{head}</div>}
                <div
                  role="option"
                  aria-selected={i === sel}
                  aria-disabled={!!r.disabled}
                  className={`pal-row${i === sel ? " sel" : ""}${r.disabled ? " disabled" : ""}${r.icon === "history" ? " mono" : ""}`}
                  onMouseMove={() => setSel(i)}
                  onClick={(e) => choose(r, e.shiftKey)}
                  title={r.disabled}
                >
                  <Icon name={r.icon} size={15} />
                  <span className="pal-label">
                    <Highlight text={r.label} indices={r.indices} />
                  </span>
                  {searching && <span className="pal-tag">{r.group}</span>}
                  {r.hint && <kbd>{r.hint}</kbd>}
                </div>
              </div>
            );
          })}
        </div>
        <div className="pal-foot">
          <span>
            <kbd>↑↓</kbd> 选择
          </span>
          <span>
            <kbd>Enter</kbd> 执行操作 / 填入命令
          </span>
          <span>
            <kbd>Shift+Enter</kbd> 直接运行命令
          </span>
        </div>
      </div>
    </div>
  );
}
