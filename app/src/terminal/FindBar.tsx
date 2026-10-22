// 终端输出里的查找框（Ctrl+Shift+F，docs/05 §7）：在回滚区里找文字，Enter 下一个、Shift+Enter 上一个、Esc 关闭。
import type { ISearchOptions, SearchAddon } from "@xterm/addon-search";
import { useEffect, useRef, useState } from "react";
import type { Theme } from "../theme/theme";

interface Props {
  search: SearchAddon;
  theme: Theme;
  onClose(): void;
}

export function FindBar({ search, theme, onClose }: Props) {
  const [text, setText] = useState("");
  const [caseSensitive, setCase] = useState(false);
  const [regex, setRegex] = useState(false);
  const [result, setResult] = useState<{ index: number; count: number } | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
    const sub = search.onDidChangeResults((r) => setResult(r.resultCount > 0 ? { index: r.resultIndex, count: r.resultCount } : { index: -1, count: 0 }));
    return () => {
      sub.dispose();
      search.clearDecorations();
    };
  }, [search]);

  const c = theme.terminal;
  const options = (incremental = false): ISearchOptions => ({
    caseSensitive,
    regex,
    incremental,
    decorations: {
      matchBackground: c.yellow,
      matchOverviewRuler: c.yellow,
      activeMatchBackground: c.selectionBackground,
      activeMatchColorOverviewRuler: c.cursor,
    },
  });

  const find = (dir: 1 | -1, incremental = false) => {
    if (!text) {
      search.clearDecorations();
      setResult(null);
      return;
    }
    try {
      if (dir > 0) search.findNext(text, options(incremental));
      else search.findPrevious(text, options());
    } catch {
      // 正则写了一半
    }
  };

  // 输入或选项变了：从当前位置增量查找
  useEffect(() => find(1, true), [text, caseSensitive, regex]);

  return (
    <div className="find-bar" role="search" onKeyDown={(e) => e.stopPropagation()}>
      <input
        ref={input}
        className="find-input"
        aria-label="在输出中查找"
        placeholder="在输出中查找"
        value={text}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            find(e.shiftKey ? -1 : 1);
          } else if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          }
        }}
      />
      <span className="find-count">{result ? (result.count === 0 ? "无结果" : `${result.index + 1}/${result.count}`) : ""}</span>
      <button type="button" className={`find-opt${caseSensitive ? " on" : ""}`} title="区分大小写" aria-pressed={caseSensitive} onClick={() => setCase((v) => !v)}>
        Aa
      </button>
      <button type="button" className={`find-opt${regex ? " on" : ""}`} title="正则表达式" aria-pressed={regex} onClick={() => setRegex((v) => !v)}>
        .*
      </button>
      <button type="button" className="find-opt" title="上一个 (Shift+Enter)" onClick={() => find(-1)}>
        ↑
      </button>
      <button type="button" className="find-opt" title="下一个 (Enter)" onClick={() => find(1)}>
        ↓
      </button>
      <button type="button" className="find-opt" title="关闭 (Esc)" onClick={onClose}>
        ×
      </button>
    </div>
  );
}
