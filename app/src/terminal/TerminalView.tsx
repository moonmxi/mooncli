// 一个标签页里的终端。行为见 docs/02 §5（渲染）和 docs/05 §3–§5（选择、复制、粘贴）。
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { UnicodeGraphemesAddon } from "@xterm/addon-unicode-graphemes";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal, type ITerminalOptions } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import { base64ToBytes } from "../backend/base64";
import { NO_PWSH, type Backend, type SessionMsg } from "../backend/types";
import { ShellMissing } from "../components/ShellMissing";
import { isAppShortcut } from "../shortcuts";
import { clamp, effectiveFontSize, LIMITS, useSettings, type AppSettings } from "../state/settings";
import { newTabId, useStore } from "../state/store";
import { useUi } from "../state/ui";
import { xtermTheme, type Theme } from "../theme/theme";
import { RemoteInput } from "./remoteInput";

const terminals = new Map<string, { term: Terminal; fit: FitAddon; search: SearchAddon }>();
export const getTerminal = (id: string) => terminals.get(id);

/**
 * 重画所有终端的字形。WebGL 渲染器的字形贴图是各终端共用的（字体、字号相同时），
 * 一个终端清空贴图后，其它终端手里的字形位置就失效了，切回去会看到乱码。
 * 所以任何一个终端清贴图，都要让全部终端重新生成一遍。
 */
export function redrawAll() {
  for (const { term } of terminals.values()) redraw(term);
}

function redraw(term: Terminal) {
  try {
    term.clearTextureAtlas();
  } catch {
    // 默认渲染器没有贴图
  }
  term.refresh(0, term.rows - 1);
}

/** 去掉每行末尾的空格（终端用空格填满整行，复制出来很难看） */
export function cleanCopy(text: string, trim = true): string {
  return trim ? text.replace(/[ \t]+$/gm, "") : text;
}

/** 终端选项：都来自设置。背景图打开时终端底色变透明（只把透明度设为 0，颜色保留给对比度计算用）。 */
export function terminalOptions(s: AppSettings, theme: Theme, transparent: boolean): ITerminalOptions {
  const t = xtermTheme(theme);
  return {
    fontFamily: s.font.family,
    fontSize: effectiveFontSize(s),
    lineHeight: s.font.lineHeight,
    fontWeight: s.font.weight as ITerminalOptions["fontWeight"],
    fontWeightBold: s.font.weightBold as ITerminalOptions["fontWeightBold"],
    cursorStyle: s.terminal.cursorStyle,
    cursorBlink: s.terminal.cursorBlink,
    scrollback: s.terminal.scrollback,
    minimumContrastRatio: s.terminal.minimumContrastRatio,
    smoothScrollDuration: s.terminal.smoothScrollMs,
    theme: transparent ? { ...t, background: `${theme.terminal.background.slice(0, 7)}00` } : t,
  };
}

interface Props {
  id: string;
  active: boolean;
  theme: Theme;
  backend: Backend;
  transparent: boolean;
}

export function TerminalView({ id, active, theme, backend, transparent }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const settings = useSettings((s) => s.settings);
  const [paste, setPaste] = useState<string | null>(null);
  /** 找不到 pwsh 7：这个标签页显示安装引导，不显示终端 */
  const [missing, setMissing] = useState(false);
  const live = useRef({ settings, paste });
  live.current = { settings, paste };

  useEffect(() => {
    const el = host.current!;
    const s0 = live.current.settings;
    const term = new Terminal({
      ...terminalOptions(s0, theme, transparent),
      allowTransparency: true,
      allowProposedApi: true,
      cursorInactiveStyle: "outline",
      drawBoldTextInBrightColors: false,
      rightClickSelectsWord: false,
      // 字形比格子宽时（有些字体里的箭头、符号）缩小到格子里，不压到旁边的字
      rescaleOverlappingGlyphs: true,
      // 方框线、块元素、Powerline 箭头由终端自己画，不依赖字体，对得严丝合缝
      customGlyphs: true,
      windowsPty: navigator.userAgent.includes("Windows") ? { backend: "conpty" } : undefined,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    term.open(el);
    // 字符宽度：优先用完整的 Unicode 15 字素簇规则（组合 emoji、国旗、肤色修饰都算一个宽字符），不行就退回 Unicode 11
    try {
      term.loadAddon(new UnicodeGraphemesAddon());
    } catch {
      term.loadAddon(new Unicode11Addon());
      term.unicode.activeVersion = "11";
    }
    try {
      const gl = new WebglAddon();
      // 显卡驱动重置、远程桌面切换时 WebGL 会丢失，退回默认渲染器
      gl.onContextLoss(() => {
        gl.dispose();
        redrawAll();
      });
      term.loadAddon(gl);
    } catch (e) {
      console.warn("mooncli: WebGL 不可用，使用默认渲染器", e);
    }
    terminals.set(id, { term, fit, search });
    fit.fit();

    const apply = useStore.getState().apply;
    const launch = useStore.getState().tabs.find((t) => t.id === id)?.launch;
    const remoteInput = new RemoteInput(term);
    /** 远程相关的消息：顺手维护"输入起点"，命令开始时把屏幕上的文字读出来 */
    const onRemote = (msg: SessionMsg): SessionMsg => {
      switch (msg.type) {
        case "remoteConnected":
        case "remoteCommandFinished":
          remoteInput.arm();
          break;
        case "remotePrompt":
          remoteInput.arm(true);
          break;
        case "remoteCommandStarted": {
          const text = remoteInput.take();
          backend.remoteCommand(id, msg.seq, text);
          return { ...msg, text };
        }
        case "remoteDisconnected": {
          remoteInput.disarm();
          const tab = useStore.getState().tabs.find((t) => t.id === id);
          if (msg.exitCode !== 0 && tab?.remote && useStore.getState().activeId === id) {
            useUi.getState().showNotice(`与 ${tab.remote.host} 的连接已断开${msg.exitCode !== null ? `（代码 ${msg.exitCode}）` : ""}`);
          }
          break;
        }
        default:
          break;
      }
      return msg;
    };
    backend
      .create({ id, cols: term.cols, rows: term.rows, cwd: useStore.getState().tabs.find((t) => t.id === id)?.initialCwd, launch }, (msg) => {
        if (msg.type === "output") {
          term.write(base64ToBytes(msg.data), () => backend.ack(id, msg.bytes));
          return;
        }
        // 其它消息等前面的输出都画完再处理，保持先后顺序
        term.write("", () => {
          apply(id, onRemote(msg));
          if (msg.type === "exited") {
            term.write(`\r\n\x1b[90m[进程已退出，代码 ${msg.code ?? "未知"}]\x1b[0m\r\n`);
            if (launch === "installPwsh") afterInstall();
          }
        });
      })
      .catch((e) => {
        if (String(e).includes(NO_PWSH)) setMissing(true);
        else term.write(`\x1b[31m启动 shell 失败：${String(e)}\x1b[0m\r\n`);
      });

    // 安装 pwsh 7 的标签页结束后：重新检测，找到了就开一个新的 pwsh 7 标签页（安装记录留着给用户看）
    const afterInstall = () =>
      backend.shellStatus().then(
        (st) => {
          if (!st.pwsh) {
            term.write("\x1b[33m还没有找到 PowerShell 7。可以关掉这个标签页再试一次，或者到 https://aka.ms/PSWindows 下载安装包。\x1b[0m\r\n");
            return;
          }
          useUi.getState().showNotice("PowerShell 7 装好了");
          useStore.getState().addTab(newTabId());
        },
        () => {},
      );

    term.onData((d) => {
      remoteInput.onInput(d);
      backend.write(id, d);
    });
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

    // ---- 复制 ----
    // 最近一次在终端里松开鼠标的位置：鼠标选完再按 Ctrl+C，提醒卡片出现在松手的地方
    let lastPointer: { x: number; y: number; at: number } | null = null;
    const onMouseUp = (e: MouseEvent) => {
      lastPointer = { x: e.clientX, y: e.clientY, at: Date.now() };
      if (live.current.settings.terminal.copyOnSelect && term.hasSelection()) copySelection(e.clientX, e.clientY);
    };
    el.addEventListener("mouseup", onMouseUp);

    /** 没有鼠标位置时，用选区末尾所在的格子算位置 */
    const selectionEndPoint = () => {
      const screen = el.querySelector(".xterm-screen")?.getBoundingClientRect();
      const sel = term.getSelectionPosition();
      if (!screen) return { x: innerWidth / 2, y: innerHeight / 3 };
      if (!sel) return { x: screen.left + screen.width / 2, y: screen.top + 24 };
      const cw = screen.width / term.cols;
      const ch = screen.height / term.rows;
      const row = clamp(sel.end.y - term.buffer.active.viewportY - 1, 0, term.rows - 1);
      return { x: screen.left + (sel.end.x - 0.5) * cw, y: screen.top + row * ch };
    };

    const copySelection = (x?: number, y?: number) => {
      const s = live.current.settings;
      const text = cleanCopy(term.getSelection(), s.terminal.trimTrailingWhitespace);
      if (!text) return;
      backend.clipboardWrite(text).catch(() => {});
      if (s.terminal.copyToast) {
        const p =
          x !== undefined && y !== undefined
            ? { x, y }
            : lastPointer && Date.now() - lastPointer.at < 15_000
              ? lastPointer
              : selectionEndPoint();
        useUi.getState().showCopyToast(p.x, p.y, text);
      }
      if (!s.terminal.copyOnSelect) term.clearSelection();
    };

    // ---- 粘贴：剪贴板从 Rust 端读，WebView 不会弹权限提示 ----
    const doPaste = (text: string) => {
      if (!text) return;
      const s = live.current.settings;
      if (s.terminal.confirmMultilinePaste && /\r?\n./.test(text) && !term.modes.bracketedPasteMode) setPaste(text);
      else term.paste(text);
    };
    const pasteFromClipboard = () => backend.clipboardRead().then(doPaste).catch(() => {});

    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      if (isAppShortcut(e, live.current.settings.keys)) return false;
      const ctrl = e.ctrlKey && !e.altKey && !e.metaKey;
      const k = e.key.toLowerCase();
      // Ctrl+C：有选中内容时复制，没有时照常发中断信号
      if (ctrl && !e.shiftKey && k === "c" && term.hasSelection()) {
        copySelection();
        return false;
      }
      if (ctrl && e.shiftKey && k === "c") {
        copySelection();
        return false;
      }
      if ((ctrl && k === "v") || (e.shiftKey && !e.ctrlKey && e.key === "Insert")) {
        e.preventDefault();
        pasteFromClipboard();
        return false;
      }
      return true;
    });

    // 其它途径的粘贴（输入法、拖放文字）也走同一套确认逻辑
    const onPaste = (e: ClipboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      doPaste(e.clipboardData?.getData("text/plain") ?? "");
    };
    el.addEventListener("paste", onPaste, true);

    // ---- 右键 ----
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const mode = live.current.settings.terminal.rightClick;
      // Shift+右键总是弹出菜单
      if (mode === "menu" || e.shiftKey) {
        const has = term.hasSelection();
        useUi.getState().openMenu(e.clientX, e.clientY, [
          { label: "复制", icon: "copy", hint: "Ctrl+C", disabled: !has, onSelect: () => copySelection(e.clientX, e.clientY) },
          { label: "粘贴", icon: "paste", hint: "Ctrl+V", onSelect: pasteFromClipboard },
          { label: "全选", icon: "select", onSelect: () => term.selectAll() },
          "separator",
          { label: "清除屏幕", icon: "eraser", onSelect: () => term.clear() },
        ]);
        return;
      }
      if (term.hasSelection()) copySelection(e.clientX, e.clientY);
      else pasteFromClipboard();
    };
    el.addEventListener("contextmenu", onContext);

    // ---- Ctrl+滚轮：只调终端字号（docs/03 §2） ----
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      e.stopPropagation();
      const dir = e.deltaY < 0 ? 1 : -1;
      useSettings.getState().update((d) => {
        d.font.size = clamp(d.font.size + dir, LIMITS.fontSize.min, LIMITS.fontSize.max);
      });
      useUi.getState().showNotice(`终端字号 ${useSettings.getState().settings.font.size}`);
    };
    el.addEventListener("wheel", onWheel, { passive: false, capture: true });

    // 附带的 Nerd Font 符号字体和系统字体加载完后，重新测量字符宽度、重画字形
    const remeasure = () => {
      term.options.fontFamily = live.current.settings.font.family;
      fit.fit();
      redrawAll();
    };
    Promise.all([document.fonts?.load(`14px "mooncli Symbols"`, ""), document.fonts?.ready]).then(remeasure, remeasure);

    return () => {
      ro.disconnect();
      clearTimeout(resizeTimer);
      el.removeEventListener("mouseup", onMouseUp);
      el.removeEventListener("paste", onPaste, true);
      el.removeEventListener("contextmenu", onContext);
      el.removeEventListener("wheel", onWheel, { capture: true });
      terminals.delete(id);
      backend.close(id);
      term.dispose();
    };
    // 终端只按 id 创建一次；外观设置在下面单独更新
  }, [id]);

  // 设置或主题变了：更新终端选项（字号变化后重新算行列数）
  useEffect(() => {
    const t = terminals.get(id);
    if (!t) return;
    const opts = terminalOptions(settings, theme, transparent);
    const before = `${t.term.options.fontSize}|${t.term.options.fontFamily}|${t.term.options.lineHeight}`;
    t.term.options = opts;
    if (before !== `${opts.fontSize}|${opts.fontFamily}|${opts.lineHeight}`) {
      requestAnimationFrame(() => {
        t.fit.fit();
        redrawAll();
      });
    }
  }, [id, settings, theme, transparent]);

  useEffect(() => {
    const t = terminals.get(id);
    if (active && t) {
      requestAnimationFrame(() => {
        t.fit.fit();
        // 后台期间别的终端可能重建过共用的字形贴图，切回来先重画一遍
        redraw(t.term);
        t.term.focus();
      });
    }
  }, [id, active]);

  const confirmPaste = (ok: boolean) => {
    const t = terminals.get(id);
    if (ok && live.current.paste && t) t.term.paste(live.current.paste);
    setPaste(null);
    t?.term.focus();
  };

  return (
    <div className={`term-layer${active ? " active" : ""}`} aria-hidden={!active}>
      <div className="term-host" ref={host} />
      {missing && <ShellMissing tabId={id} backend={backend} />}
      {paste !== null && <PasteConfirm text={paste} onDone={confirmPaste} />}
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
      <pre className="paste-preview">
        {lines.slice(0, 4).join("\n")}
        {lines.length > 4 ? "\n…" : ""}
      </pre>
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
