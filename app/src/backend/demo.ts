// 演示后端：直接用浏览器打开界面时用，模拟一个简单的 pwsh，方便调界面。
// 它直接产出界面消息（真后端里这些由 Rust 解析 shell 集成标记得到）。
import { bytesToBase64 } from "./base64";
import type { Backend, CreateOptions, SessionMsg } from "./types";

const enc = new TextEncoder();
const ESC = "\x1b";
const c = (code: string, s: string) => `${ESC}[${code}m${s}${ESC}[0m`;

interface Fake {
  emit: (m: SessionMsg) => void;
  line: string;
  cwd: string;
  mode: "prompt" | "running" | "ssh";
  timers: number[];
}

/** 检查字符显示：emoji、箭头、方框线、Nerd Font 图标、中文 */
export const GLYPHS = [
  "emoji      😀 🎉 🚀 ✅ ❌ ⚠️ 🔥 👍🏽 👨‍👩‍👧 🇨🇳 ❤️",
  "箭头       ← ↑ → ↓ ↔ ⇒ ➜ ❯ ➤ ▶ ◀ ⟶ ↳",
  "方框线     ┌─┬─┐ │ ├─┼─┤ └─┴─┘ ╭─╮ ╰─╯ ━ ┃ ░▒▓█",
  "提示符图标 \ue0b0 \ue0b2 \ue0b4 \ue0b6  \ue725  \uf113  \ue718  \ue7a8  \uf07b  \uf489  \uf0e7",
  "盲文/点阵  ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏  ✳ ✻ ● ◐ ◓",
  "中英混排   mooncli 是一个浅色优先的终端，支持 CJK 宽字符对齐。",
].join("\n");

const FILES: Record<string, string[]> = {
  "C:\\Users\\moon\\code\\mooncli": ["app", "crates", "design", "docs", "shell", "Cargo.toml", "README.md"],
  "C:\\Users\\moon\\code\\mooncli\\docs": ["00-vision.md", "01-architecture.md", "08-art-direction.md", "09-roadmap.md"],
};

function out(f: Fake, s: string) {
  const bytes = enc.encode(s);
  f.emit({ type: "output", data: bytesToBase64(bytes), bytes: bytes.length });
}

function prompt(f: Fake) {
  f.mode = "prompt";
  const short = f.cwd.replace("C:\\Users\\moon", "~");
  out(f, `${c("90", "PS ")}${c("34", short)} ${c("35", "main*")}${c("90", "> ")}`);
  f.emit({ type: "state", state: "prompt" });
}

function finish(f: Fake, text: string, code: number) {
  f.emit({ type: "commandFinished", text, exitCode: code });
  prompt(f);
}

/** 演示用的历史：浏览器里敲过的命令，加几条示例 */
const demoHistory: string[] = ["git status", "git push origin main", "cargo test -p mooncli-history", "pnpm tauri dev", "ssh box"];

function run(f: Fake, text: string) {
  if (text.trim()) demoHistory.unshift(text.trim());
  const [cmd, ...args] = text.trim().split(/\s+/);
  f.emit({ type: "commandStarted", text });
  f.emit({ type: "state", state: "running" });
  const nl = "\r\n";
  switch (cmd?.toLowerCase()) {
    case "help":
      out(f, `演示后端支持：${c("1", "ls  cd <目录>  git status  colors  glyphs  fail  sleep  claude  ssh box  clear  help")}${nl}`);
      out(f, `这是浏览器里的模拟 shell，在 Tauri 窗口里运行时会换成真正的 pwsh。${nl}`);
      return finish(f, text, 0);
    case "ls":
    case "dir":
    case "get-childitem": {
      const items = FILES[f.cwd] ?? [];
      out(f, items.map((n) => (n.includes(".") ? n : c("34;1", n))).join("  ") + nl);
      return finish(f, text, 0);
    }
    case "cd":
    case "set-location": {
      const target = args[0] ?? "~";
      if (target === "..") f.cwd = f.cwd.split("\\").slice(0, -1).join("\\") || "C:\\";
      else if (target === "~") f.cwd = "C:\\Users\\moon";
      else if (FILES[`${f.cwd}\\${target}`]) f.cwd = `${f.cwd}\\${target}`;
      else {
        out(f, c("31", `Set-Location: 找不到路径 '${target}'。`) + nl);
        return finish(f, text, 1);
      }
      f.emit({ type: "cwd", host: "MOON-PC", path: f.cwd });
      return finish(f, text, 0);
    }
    case "git":
      out(f, `On branch main${nl}Changes not staged for commit:${nl}        ${c("31", "modified:   docs/06-smart-tabs.md")}${nl}`);
      return finish(f, text, 0);
    case "colors": {
      const names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
      out(f, names.map((n, i) => c(`${30 + i}`, n.padEnd(8))).join("") + nl);
      out(f, names.map((n, i) => c(`${90 + i}`, ("b" + n).padEnd(8))).join("") + nl);
      return finish(f, text, 0);
    }
    case "glyphs":
      out(f, GLYPHS.replace(/\n/g, nl) + nl);
      return finish(f, text, 0);
    case "fail":
      out(f, c("31", "fail: 这是一条故意失败的命令。") + nl);
      return finish(f, text, 1);
    case "clear":
    case "cls":
      out(f, `${ESC}[2J${ESC}[H`);
      return finish(f, text, 0);
    case "sleep": {
      out(f, `睡 3 秒……${nl}`);
      f.timers.push(window.setTimeout(() => finish(f, text, 0), 3000));
      f.mode = "running";
      return;
    }
    case "claude": {
      f.mode = "running";
      out(f, `${c("38;5;173", "✳")} Claude Code（演示）${nl}${nl}`);
      f.emit({ type: "title", text: "重构标签页布局", activity: "✳", informative: true });
      const lines = ["读取 docs/06-smart-tabs.md", "修改 app/src/components/Sidebar.tsx", "运行 pnpm test"];
      lines.forEach((l, i) =>
        f.timers.push(window.setTimeout(() => out(f, `  ${c("90", "●")} ${l}${nl}`), 700 * (i + 1))),
      );
      f.timers.push(
        window.setTimeout(() => {
          out(f, `${nl}需要你确认：是否允许运行 ${c("1", "pnpm test")}？(y/n) `);
          out(f, "\x07");
          f.emit({ type: "bell" });
        }, 700 * (lines.length + 1)),
      );
      return;
    }
    case "ssh": {
      f.mode = "ssh";
      out(f, `连接 ${args[0] ?? "box"}……（演示）${nl}`);
      f.emit({ type: "cwd", host: args[0] ?? "box", path: "/root" });
      out(f, `root@${args[0] ?? "box"}:~# `);
      return;
    }
    case undefined:
    case "":
      return prompt(f);
    default:
      out(f, c("31", `${cmd}: 无法识别为 cmdlet、函数、脚本文件或可运行程序的名称。（演示后端，输入 help 查看支持的命令）`) + nl);
      return finish(f, text, 1);
  }
}

function input(f: Fake, data: string) {
  for (const ch of data) {
    if (f.mode === "running") {
      if (ch === "\x03") {
        f.timers.forEach(clearTimeout);
        f.timers = [];
        out(f, "^C\r\n");
        f.emit({ type: "title", text: "", activity: null, informative: false });
        finish(f, "", 130);
      } else if (ch === "\r") out(f, "\r\n");
      else out(f, ch);
      continue;
    }
    if (ch === "\r") {
      out(f, "\r\n");
      const text = f.line;
      f.line = "";
      if (f.mode === "ssh") {
        if (text.trim() === "exit") {
          out(f, "logout\r\n");
          finish(f, "ssh box", 0);
        } else {
          out(f, `${text ? `bash: ${text}: 演示里的远程 shell 不执行命令\r\n` : ""}root@box:~# `);
        }
        continue;
      }
      if (text.trim()) run(f, text);
      else prompt(f);
    } else if (ch === "\x7f") {
      if (f.line) {
        f.line = [...f.line].slice(0, -1).join("");
        out(f, "\b \b");
      }
    } else if (ch === "\x03") {
      f.line = "";
      out(f, "^C\r\n");
      prompt(f);
    } else if (ch === "\t" && f.mode === "ssh") {
      // 演示：ssh 里的 TAB 原样到达远程 shell
      out(f, "\x07");
    } else if (ch >= " " && ch !== "\x1b") {
      f.line += ch;
      out(f, ch);
    }
  }
}

const SETTINGS_KEY = "mooncli.demo.settings";

export function demoBackend(): Backend {
  const sessions = new Map<string, Fake>();
  const backgrounds = new Map<string, string>();
  return {
    kind: "demo",
    async loadSettings() {
      try {
        return JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}");
      } catch {
        return {};
      }
    },
    async saveSettings(v) {
      try {
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(v));
      } catch {
        // 浏览器不让存就算了
      }
    },
    clipboardRead: () => navigator.clipboard.readText().catch(() => ""),
    clipboardWrite: (t) => navigator.clipboard.writeText(t).catch(() => {}),
    async backgroundSave(fileName, data) {
      const ext = fileName.split(".").pop()?.toLowerCase() ?? "png";
      backgrounds.set(fileName, `data:image/${ext === "jpg" ? "jpeg" : ext};base64,${data}`);
      return fileName;
    },
    async backgroundLoad(name) {
      const d = backgrounds.get(name);
      if (!d) throw new Error(`找不到背景图 ${name}`);
      return d;
    },
    backgroundList: async () => [...backgrounds.keys()],
    backgroundDelete: async (name) => void backgrounds.delete(name),
    diagnostics: async () => ({
      appVersion: "0.1.0（演示）",
      os: "browser",
      osVersion: "",
      shellPath: null,
      shellKind: "演示后端",
      integrationEnabled: true,
      bundledConpty: false,
      settingsPath: "浏览器 localStorage",
      historyPath: "内存",
      historyCount: demoHistory.length,
      backgroundsDir: "内存",
    }),
    sessionStats: async () => ({ outputBytes: 1024, trustedMarks: 4, untrustedMarks: 0, privateMessages: 1, ageMs: 1000 }),
    async historySearch(text, opts) {
      const q = text.toLowerCase();
      const seen = new Set<string>();
      return demoHistory
        .filter((t) => (seen.has(t) ? false : (seen.add(t), t.toLowerCase().includes(q))))
        .slice(0, opts?.limit ?? 50)
        .map((t, i) => ({ id: i, text: t, count: 1, lastUsed: Date.now(), lastCwd: null, lastExit: 0, matched: [] }));
    },
    async create(opts: CreateOptions, onMsg) {
      const f: Fake = { emit: onMsg, line: "", cwd: "C:\\Users\\moon\\code\\mooncli", mode: "prompt", timers: [] };
      sessions.set(opts.id, f);
      onMsg({ type: "env", pwsh: "7.6.7（演示）", host: "MOON-PC", admin: false });
      onMsg({ type: "cwd", host: "MOON-PC", path: f.cwd });
      out(f, `${c("90", "mooncli 演示后端 · 输入 help 查看可用命令")}\r\n`);
      prompt(f);
    },
    write(id, data) {
      const f = sessions.get(id);
      if (f) input(f, data);
    },
    resize() {},
    ack() {},
    close(id) {
      sessions.get(id)?.timers.forEach(clearTimeout);
      sessions.delete(id);
    },
    window: { minimize() {}, toggleMaximize() {}, close() {} },
  };
}
