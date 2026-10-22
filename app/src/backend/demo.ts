// 演示后端：直接用浏览器打开界面时用，模拟一个简单的 pwsh，方便调界面。
// 它直接产出界面消息（真后端里这些由 Rust 解析 shell 集成标记得到）。
import { bytesToBase64 } from "./base64";
import { NO_PWSH, type Backend, type CreateOptions, type HistoryHit, type HistoryQuery, type SessionMsg, type UsageReport } from "./types";

const enc = new TextEncoder();
const ESC = "\x1b";
const c = (code: string, s: string) => `${ESC}[${code}m${s}${ESC}[0m`;

// 演示后端的「一键接入」状态（只在内存里，刷新页面复位）
const demoIntegration = { claude: false, codex: false };
function demoIntegrationChanges(target: "claude" | "codex", action: "integrate" | "unintegrate"): string[] {
  if (target === "claude") {
    return action === "integrate"
      ? ["新增 statusLine：mooncli-bridge claude-statusline（状态栏显示模型、上下文与额度）", "hooks 追加 mooncli-bridge（SessionStart、UserPromptSubmit、PermissionRequest、Notification、PostToolUse、Stop、SessionEnd），你原有的 hooks 不动"]
      : ["settings.json 恢复成接入前的原文"];
  }
  return action === "integrate" ? ["新增 notify：mooncli-bridge codex-notify"] : ["删除 notify（mooncli-bridge）"];
}

interface Fake {
  emit: (m: SessionMsg) => void;
  line: string;
  cwd: string;
  mode: "prompt" | "running" | "ssh";
  timers: number[];
  /** 远程命令编号 */
  seq: number;
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

interface DemoRun {
  id: number;
  text: string;
  cwd: string | null;
  exit: number | null;
  at: number;
  duration: number | null;
  host: string;
  shell: string;
}

const HOUR = 3_600_000;
const REPO = "C:\\Users\\moon\\code\\mooncli";
let runId = 0;
const sample = (text: string, hoursAgo: number, exit: number | null = 0, cwd: string | null = REPO, host = "MOON-PC", shell = "pwsh", duration = 800): DemoRun => ({
  id: ++runId,
  text,
  cwd,
  exit,
  at: Date.now() - hoursAgo * HOUR,
  duration,
  host,
  shell,
});

/** 演示用的历史：几天里的示例，加上浏览器里敲过的命令 */
const demoHistory: DemoRun[] = [
  sample("git status", 0.1),
  sample("pnpm tauri dev", 0.5, 0, REPO, "MOON-PC", "pwsh", 3_600_000),
  sample("cargo test -p mooncli-history", 1, 101, REPO, "MOON-PC", "pwsh", 42_000),
  sample("cargo test -p mooncli-history", 1.2),
  sample("git push origin main", 3),
  sample("git push origin main", 26),
  sample("ssh deploy@prod-web", 27, 0, REPO, "MOON-PC", "pwsh", 2100),
  sample("sudo systemctl restart nginx", 27, null, "/etc/nginx", "prod-web", "ssh", 1500),
  sample("tail -f /var/log/nginx/error.log", 27.1, null, "/var/log/nginx", "prod-web", "ssh", 95_000),
  sample("docker compose ps", 50, null, "/srv/app", "prod-web", "ssh", 900),
  sample("npm run build", 80, 1, "C:\\Users\\moon\\code\\blog"),
  sample("winget upgrade --all", 120, 0, "C:\\Users\\moon"),
].sort((a, b) => b.at - a.at);
// 测性能用：localStorage 里设了 mooncli.demo.bigHistory 时，再加几千条
try {
  const n = Number(localStorage.getItem("mooncli.demo.bigHistory") ?? 0);
  const verbs = ["git commit -m", "cargo build -p", "npm run", "docker logs", "kubectl get pods -n", "python manage.py"];
  for (let i = 0; i < n; i++) demoHistory.push(sample(`${verbs[i % verbs.length]} item-${i % 997}`, 1 + i / 40, i % 9 ? 0 : 1));
  if (n) demoHistory.sort((a, b) => b.at - a.at);
} catch {
  // 没有 localStorage 就算了
}
const demoFavorites = new Set(["git push origin main", "pnpm tauri dev"]);

function remember(text: string, extra: Partial<DemoRun> = {}) {
  if (!text.trim()) return;
  demoHistory.unshift({ ...sample(text.trim(), 0), exit: null, duration: null, ...extra });
}

/** 简单的子串匹配，给出高亮位置；匹配不上返回 null */
function demoMatch(text: string, q: string): number[] | null {
  if (!q) return [];
  const i = text.toLowerCase().indexOf(q.toLowerCase());
  if (i < 0) return null;
  const start = [...text.slice(0, i)].length;
  return [...q].map((_, k) => start + k);
}

function demoQuery(q: HistoryQuery): HistoryHit[] {
  const query = q.text.trim();
  const rows = demoHistory.filter((r) => {
    if (q.scope?.kind === "cwd" && r.cwd !== q.scope.value) return false;
    if (q.host && r.host !== q.host) return false;
    if (q.successOnly && r.exit !== 0) return false;
    if (q.failedOnly && !(r.exit !== null && r.exit !== 0)) return false;
    if (q.favoritesOnly && !demoFavorites.has(r.text)) return false;
    return demoMatch(r.text, query) !== null;
  });
  const hits: HistoryHit[] = [];
  const byText = new Map<string, HistoryHit>();
  for (const r of rows) {
    const old = q.dedupe === false ? undefined : byText.get(r.text);
    if (old) {
      old.count++;
      continue;
    }
    const h: HistoryHit = {
      id: r.id,
      text: r.text,
      count: 1,
      lastUsed: r.at,
      lastCwd: r.cwd,
      lastExit: r.exit,
      durationMs: r.duration,
      host: r.host,
      favorite: demoFavorites.has(r.text),
      matched: demoMatch(r.text, query) ?? [],
    };
    byText.set(r.text, h);
    hits.push(h);
  }
  if (q.sort !== "recent" && query) hits.sort((a, b) => b.count - a.count);
  return hits.slice(0, q.limit ?? 200);
}

function run(f: Fake, text: string) {
  remember(text, { cwd: f.cwd });
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
      // 演示连接过程：连接中（计时）→ 连上（这条命令算完成）→ 之后记录远程命令
      const host = (args[0] ?? "box").replace(/^.*@/, "");
      f.emit({ type: "remoteConnecting", target: { via: "ssh", host, user: "root" } });
      out(f, `root@${host}'s password: `);
      f.timers.push(
        window.setTimeout(() => {
          f.mode = "ssh";
          out(f, `${nl}Welcome to Ubuntu 24.04 LTS（演示）${nl}${nl}`);
          f.emit({ type: "remoteConnected", target: { via: "ssh", host, user: "root" } });
          f.emit({ type: "cwd", host, path: "/root" });
          out(f, `root@${host}:~# `);
        }, 1200),
      );
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
        const seq = ++f.seq;
        f.emit({ type: "remoteCommandStarted", seq });
        if (text.trim() === "exit") {
          out(f, "logout\r\n");
          f.emit({ type: "remoteCommandFinished", seq, exitCode: null });
          f.emit({ type: "remoteDisconnected", exitCode: 0 });
          prompt(f);
        } else {
          const ms = text.startsWith("sleep") ? 2500 : 150;
          f.mode = "running";
          f.timers.push(
            window.setTimeout(() => {
              f.mode = "ssh";
              if (text.trim() && !text.startsWith("sleep")) out(f, `${text.trim()}：演示里的远程 shell 不真正执行命令\r\n`);
              f.emit({ type: "remoteCommandFinished", seq, exitCode: null });
              out(f, "root@box:~# ");
            }, ms),
          );
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

const demoBackups = [
  { name: "settings-1-auto.json", at: Date.now() - 3_600_000, bytes: 1350, reason: "auto" },
  { name: "settings-2-auto.json", at: Date.now() - 26 * 3_600_000, bytes: 1310, reason: "auto" },
];

function demoNoPwsh(): boolean {
  try {
    return !!localStorage.getItem("mooncli.demo.noPwsh");
  } catch {
    return false;
  }
}

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
    shellStatus: async () => ({
      pwsh: demoNoPwsh() ? null : "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
      configuredMissing: false,
      windowsPowershell: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
      winget: true,
    }),
    openUrl: async (url) => void window.open(url, "_blank"),
    // 演示后端只模拟数据管理的界面，不真的读写文件
    dataOverview: async () => ({
      configDir: "C:\\Users\\moon\\AppData\\Roaming\\dev.moonmxi.mooncli",
      dataDir: "C:\\Users\\moon\\AppData\\Local\\dev.moonmxi.mooncli",
      exportDir: "C:\\Users\\moon\\Downloads",
      settingsBytes: (localStorage.getItem(SETTINGS_KEY) ?? "").length,
      backgrounds: [...backgrounds].map(([name, d]) => ({ name, bytes: Math.round((d.length * 3) / 4) })),
      backups: demoBackups,
      backupsBytes: demoBackups.reduce((a, b) => a + b.bytes, 0),
      historyBytes: demoHistory.length * 180,
      historyCount: demoHistory.length,
    }),
    async dataBackupNow() {
      const name = `settings-${Date.now()}-manual.json`;
      demoBackups.unshift({ name, at: Date.now(), bytes: 1200, reason: "manual" });
      return name;
    },
    dataRestoreBackup: async () => JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}"),
    async settingsReset() {
      demoBackups.unshift({ name: `settings-${Date.now()}-before-reset.json`, at: Date.now(), bytes: 1200, reason: "before-reset" });
      localStorage.removeItem(SETTINGS_KEY);
      return {};
    },
    dataExport: async () => `C:\\Users\\moon\\Downloads\\mooncli-backup-${Date.now()}.zip`,
    dataImport: async () => ({ settings: null, backgrounds: 0, historyAdded: 0 }),
    dataReveal: async () => {},
    async historyPrune(days) {
      const cutoff = days === 0 ? Infinity : Date.now() - days * 86_400_000;
      const before = demoHistory.length;
      for (let i = demoHistory.length - 1; i >= 0; i--) if (demoHistory[i].at < cutoff) demoHistory.splice(i, 1);
      return before - demoHistory.length;
    },
    sessionStats: async () => ({ outputBytes: 1024, trustedMarks: 4, untrustedMarks: 0, privateMessages: 1, ageMs: 1000 }),
    // 演示数据：和 docs/07 第 5 节的卡片示意一致，时间按打开页面的时刻现算
    agentUsage: async () => {
      const now = Math.floor(Date.now() / 1000);
      return {
        sessions: [
          {
            kind: "claude",
            sessionId: "demo-claude-1",
            cwd: "C:\\Users\\moon\\code\\mooncli",
            model: "Opus",
            context: { usedTokens: 126_000, windowTokens: 1_000_000 },
            state: "waiting",
            updatedAt: now - 20,
            source: "statusLine",
          },
          {
            kind: "codex",
            sessionId: "demo-codex-1",
            cwd: "C:\\Users\\moon\\code\\other-project",
            model: "gpt-5-codex",
            context: { usedTokens: 171_500, windowTokens: 475_000 },
            state: "done",
            updatedAt: now - 900,
            source: "sessionFile",
          },
        ],
        accounts: [
          {
            kind: "claude",
            primary: { usedPercent: 38, windowMinutes: 300, resetsAt: now + 2 * 3600 + 18 * 60, estimated: false },
            secondary: { usedPercent: 21, windowMinutes: 10080, resetsAt: now + 4 * 86400, estimated: false },
            updatedAt: now - 20,
            source: "statusLine",
          },
          {
            kind: "codex",
            primary: { usedPercent: 27, windowMinutes: 300, resetsAt: now + 3600 + 42 * 60, estimated: false },
            secondary: { usedPercent: 9, windowMinutes: 10080, resetsAt: now + 5 * 86400, estimated: false },
            plan: "plus",
            updatedAt: now - 60,
            source: "live",
          },
        ],
      } satisfies UsageReport;
    },
    agentRefreshLive: async () => {},
    agentWindow: async () => {},
    agentBridgePath: async () => "C:\\Program Files\\mooncli\\mooncli-bridge.exe",
    agentIntegrationStatus: async () => ({
      claude: demoIntegration.claude,
      codex: demoIntegration.codex,
      claudeSettingsPath: "C:\\Users\\moon\\.claude\\settings.json",
      codexConfigPath: "C:\\Users\\moon\\.codex\\config.toml",
      codexExe: "C:\\Users\\moon\\AppData\\Roaming\\npm\\codex.exe",
      bridgeExists: true,
    }),
    agentIntegrationPreview: async (target, action) => ({ changes: demoIntegrationChanges(target, action) }),
    agentIntegrationApply: async (target, action) => {
      const changes = demoIntegrationChanges(target, action);
      demoIntegration[target] = action === "integrate";
      return { changes };
    },
    historySearch: async (text, opts) => demoQuery({ text, limit: opts?.limit ?? 50 }),
    historyQuery: async (q) => demoQuery(q),
    async historyDetail(text) {
      const runs = demoHistory.filter((r) => r.text === text);
      const durations = runs.map((r) => r.duration).filter((d): d is number => d !== null);
      return {
        summary: {
          count: runs.length,
          ok: runs.filter((r) => r.exit === 0).length,
          failed: runs.filter((r) => r.exit !== null && r.exit !== 0).length,
          firstUsed: runs.length ? Math.min(...runs.map((r) => r.at)) : null,
          avgDurationMs: durations.length ? Math.round(durations.reduce((a, b) => a + b, 0) / durations.length) : null,
          dirs: new Set(runs.map((r) => r.cwd)).size,
        },
        runs: runs.slice(0, 20).map((r) => ({
          id: r.id,
          startedAt: r.at,
          cwd: r.cwd,
          exitCode: r.exit,
          durationMs: r.duration,
          host: r.host,
          shell: r.shell,
          imported: false,
        })),
      };
    },
    async historyHosts() {
      const m = new Map<string, { count: number; remote: boolean }>();
      for (const r of demoHistory) {
        const e = m.get(r.host) ?? { count: 0, remote: r.shell !== "pwsh" };
        e.count++;
        m.set(r.host, e);
      }
      return [...m].map(([host, e]) => ({ host, ...e })).sort((a, b) => b.count - a.count);
    },
    async historyFavorite(text, on) {
      if (on) demoFavorites.add(text);
      else demoFavorites.delete(text);
    },
    async historyDelete(text, id) {
      let n = 0;
      for (let i = demoHistory.length - 1; i >= 0; i--) {
        if (id !== undefined ? demoHistory[i].id === id : demoHistory[i].text === text) {
          demoHistory.splice(i, 1);
          n++;
        }
      }
      if (id === undefined) demoFavorites.delete(text);
      return n;
    },
    remoteCommand(_id, _seq, text) {
      remember(text, { host: "box", shell: "ssh", cwd: "/root" });
    },
    async create(opts: CreateOptions, onMsg) {
      // 演示"电脑上没有 pwsh 7"：localStorage 里设了 mooncli.demo.noPwsh
      if (!opts.launch && demoNoPwsh()) throw new Error(`${NO_PWSH}: 没有找到 PowerShell 7（演示）`);
      const f: Fake = { emit: onMsg, line: "", cwd: "C:\\Users\\moon\\code\\mooncli", mode: "prompt", timers: [], seq: 0 };
      sessions.set(opts.id, f);
      if (opts.launch === "installPwsh") {
        const lines = ["正在用 winget 从微软官方源安装 PowerShell 7，可能需要一两分钟……", "已找到 PowerShell [Microsoft.PowerShell] 版本 7.6.7", "正在下载 …  ██████████████████████  110 MB / 110 MB", "已成功安装"];
        lines.forEach((l, i) => f.timers.push(window.setTimeout(() => out(f, `${l}\r\n`), 500 * (i + 1))));
        f.timers.push(
          window.setTimeout(() => {
            try {
              localStorage.removeItem("mooncli.demo.noPwsh");
            } catch {
              // 没有就算了
            }
            onMsg({ type: "exited", code: 0 });
          }, 500 * (lines.length + 1)),
        );
        return;
      }
      onMsg({ type: "env", pwsh: opts.launch === "windowsPowerShell" ? "5.1（演示）" : "7.6.7（演示）", host: "MOON-PC", admin: false });
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
    onSettingsFile: () => () => {},
    attention: () => {},
    window: { minimize() {}, toggleMaximize() {}, close() {} },
  };
}
