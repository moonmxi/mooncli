// 一个标签页里的终端。行为见 docs/02 §5（渲染）和 docs/05 §3–§5（选择、复制、粘贴）。
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import { base64ToBytes } from "../backend/base64";
import type { Backend } from "../backend/types";
import { useStore } from "../state/store";
import { xtermTheme, type Theme } from "../theme/theme";
import { isAppShortcut } from "../shortcuts";

export const TERMINAL_FONT = '"Cascadia Code", "JetBrains Mono", "Sarasa Mono SC", "Microsoft YaHei UI", Consolas, monospace';
export const TERMINAL_FONT_SIZE = 14;

const terminals = new Map<string, { term: Terminal; fit: FitAddon; search: SearchAddon }>();
export const getTerminal = (id: string) => terminals.get(id);

/** 去掉每行末尾的空格（终端用空格填满整行，复制出来很难看） */
export function cleanCopy(text: string): string {
  return text.replace(/[ \t]+$/gm, "");
}

interface Props {
  id: string;
  active: boolean;
  theme: Theme;
  backend: Backend;
}

export function TerminalView({ id, active, theme, backend }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [paste, setPaste] = useState<string | null>(null);
  const pasteRef = useRef<string | null>(null);
  pasteRef.current = paste;

  useEffect(() => {
    const el = host.current!;
    const term = new Terminal({
      fontFamily: TERMINAL_FONT,
      fontSize: TERMINAL_FONT_SIZE,
      lineHeight: 1.2,
      cursorStyle: "bar",
      cursorBlink: true,
      cursorInactiveStyle: "outline",
      scrollback: 10_000,
      minimumContrastRatio: 4.5,
      smoothScrollDuration: 120,
      allowProposedApi: true,
      drawBoldTextInBrightColors: false,
      rightClickSelectsWord: false,
      theme: xtermTheme(theme),
      windowsPty: navigator.userAgent.includes("Windows") ? { backend: "conpty" } : undefined,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new Unicode11Addon());
    term.loadAddon(new WebLinksAddon());
    term.unicode.activeVersion = "11";
    term.open(el);
    try {
      const gl = new WebglAddon();
      // 显卡驱动重置、远程桌面切换时 WebGL 会丢失，退回默认渲染器
      gl.onContextLoss(() => gl.dispose());
      term.loadAddon(gl);
    } catch (e) {
      console.warn("mooncli: WebGL 不可用，使用默认渲染器", e);
    }
    terminals.set(id, { term, fit, search });
    fit.fit();

    const apply = useStore.getState().apply;
    backend
      .create({ id, cols: term.cols, rows: term.rows }, (msg) => {
        if (msg.type === "output") {
          term.write(base64ToBytes(msg.data), () => backend.ack(id, msg.bytes));
          return;
        }
        // 其它消息等前面的输出都画完再处理，保持先后顺序
        term.write("", () => {
          apply(id, msg);
          if (msg.type === "exited") {
            term.write(`\r\n\x1b[90m[进程已退出，代码 ${msg.code ?? "未知"}]\x1b[0m\r\n`);
          }
        });
      })
      .catch((e) => term.write(`\x1b[31m启动 shell 失败：${String(e)}\x1b[0m\r\n`));

    term.onData((d) => backend.write(id, d));
    term.onBinary((d) => backend.write(id, d));

    let resizeTimer = 0;
    term.onResize(({ cols, rows }) => {
      // 拖动窗口或侧栏时防抖，避免 pwsh 反复重绘（docs/06 §1.1）
      clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => backend.resize(id, cols, rows), 50);
    });
    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        if (el.offsetWidth > 0) fit.fit();
      });
    });
    ro.observe(el);

    const copySelection = () => {
      const text = cleanCopy(term.getSelection());
      if (text) navigator.clipboard.writeText(text).catch(() => {});
      term.clearSelection();
    };

    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      if (isAppShortcut(e)) return false;
      const ctrl = e.ctrlKey && !e.altKey && !e.metaKey;
      // Ctrl+C：有选中内容时复制，没有时照常发中断信号
      if (ctrl && !e.shiftKey && e.key.toLowerCase() === "c" && term.hasSelection()) {
        copySelection();
        return false;
      }
      if (ctrl && e.shiftKey && e.key.toLowerCase() === "c") {
        copySelection();
        return false;
      }
      // 粘贴交给浏览器的 paste 事件（xterm 会处理括号粘贴），这里只是不让 ^V 发出去
      if ((ctrl && e.key.toLowerCase() === "v") || (e.shiftKey && e.key === "Insert")) return false;
      return true;
    });

    // 多行粘贴确认：程序没开括号粘贴、内容又有多行时先问一下（docs/05 §5）
    const onPaste = (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData("text/plain") ?? "";
      if (/\r?\n./.test(text) && !term.modes.bracketedPasteMode) {
        e.preventDefault();
        e.stopPropagation();
        setPaste(text);
      }
    };
    el.addEventListener("paste", onPaste, true);

    // 右键：有选中就复制，没有就粘贴
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
      if (term.hasSelection()) copySelection();
      else
        navigator.clipboard
          .readText()
          .then((t) => (/\r?\n./.test(t) && !term.modes.bracketedPasteMode ? setPaste(t) : term.paste(t)))
          .catch(() => {});
    };
    el.addEventListener("contextmenu", onContext);

    // 字体加载完后重新测量字符宽度
    document.fonts?.ready.then(() => {
      term.options.fontFamily = TERMINAL_FONT;
      fit.fit();
    });

    return () => {
      ro.disconnect();
      clearTimeout(resizeTimer);
      el.removeEventListener("paste", onPaste, true);
      el.removeEventListener("contextmenu", onContext);
      terminals.delete(id);
      backend.close(id);
      term.dispose();
    };
    // 终端只按 id 创建一次；主题和激活状态在下面单独处理
  }, [id]);

  useEffect(() => {
    const t = terminals.get(id);
    if (t) t.term.options.theme = xtermTheme(theme);
  }, [id, theme]);

  useEffect(() => {
    const t = terminals.get(id);
    if (active && t) {
      requestAnimationFrame(() => {
        t.fit.fit();
        t.term.focus();
      });
    }
  }, [id, active]);

  const confirmPaste = (ok: boolean) => {
    const t = terminals.get(id);
    if (ok && pasteRef.current && t) {
      const text = pasteRef.current;
      // 大段内容在 Rust 端分块写入
      t.term.paste(text);
    }
    setPaste(null);
    t?.term.focus();
  };

  return (
    <div className={`term-layer${active ? " active" : ""}`} aria-hidden={!active}>
      <div className="term-host" ref={host} />
      {paste !== null && (
        <PasteConfirm text={paste} onDone={confirmPaste} />
      )}
    </div>
  );
}

function PasteConfirm({ text, onDone }: { text: string; onDone: (ok: boolean) => void }) {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <div
      className="paste-confirm"
      role="alertdialog"
      aria-label="确认多行粘贴"
      tabIndex={-1}
      ref={ref}
      onKeyDown={(e) => {
        if (e.key === "Enter") onDone(true);
        if (e.key === "Escape") onDone(false);
        e.stopPropagation();
      }}
    >
      <div className="paste-head">
        <strong>将粘贴 {lines.length} 行</strong>
        <span>当前程序没有开启括号粘贴，可能会逐行执行</span>
      </div>
      <pre className="paste-preview">{lines.slice(0, 4).join("\n")}{lines.length > 4 ? "\n…" : ""}</pre>
      <div className="paste-actions">
        <button type="button" className="btn primary" onClick={() => onDone(true)}>
          粘贴 <kbd>Enter</kbd>
        </button>
        <button type="button" className="btn" onClick={() => onDone(false)}>
          取消 <kbd>Esc</kbd>
        </button>
      </div>
    </div>
  );
}
