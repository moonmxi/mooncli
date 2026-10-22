// 标签页标题规则，见 docs/06-smart-tabs.md §3：用户改的 > 程序设置的 > 自动生成。
import type { ProgramKind } from "../backend/types";

export interface TitleParts {
  user?: string;
  /** 程序设置的标题（已去掉状态符号），只在命令运行期间有效 */
  program?: string;
  activity?: string;
}

export interface TitleContext {
  title: TitleParts;
  cwd?: string;
  profile: string;
  running?: { text?: string; kind?: ProgramKind };
  remote?: { host: string; user?: string; via?: string };
}

export function dirName(path?: string): string | undefined {
  if (!path) return undefined;
  const trimmed = path.replace(/[\\/]+$/, "");
  if (trimmed === "" ) return "/";
  if (/^[A-Za-z]:$/.test(trimmed)) return trimmed + "\\";
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || trimmed;
}

/** 从命令文本里猜程序名：`npm run dev` → npm，`& "C:\x\claude.exe"` → claude */
export function programName(command?: string): string | undefined {
  if (!command) return undefined;
  let s = command.trim().replace(/^&\s*/, "");
  const quoted = s.match(/^["']([^"']+)["']/);
  const first = quoted ? quoted[1] : s.split(/\s+/)[0];
  if (!first) return undefined;
  const base = first.split(/[\\/]/).pop()!;
  s = base.replace(/\.(exe|cmd|bat|ps1)$/i, "");
  return s || undefined;
}

export function autoTitle(ctx: TitleContext): string {
  const dir = ctx.remote ? ctx.remote.host : dirName(ctx.cwd) ?? ctx.profile;
  const prog = programName(ctx.running?.text);
  return prog ? `${prog} · ${dir}` : dir;
}

export function displayTitle(ctx: TitleContext): string {
  if (ctx.title.user) return ctx.title.user;
  if (ctx.title.program && ctx.running) return ctx.title.program;
  return autoTitle(ctx);
}

/** 第二行：目录 · 分支 · 程序（或 user@host · ssh） */
export function metaLine(ctx: TitleContext & { git?: string }, full = false): string {
  const parts: string[] = [];
  if (ctx.remote) {
    parts.push(ctx.remote.user ? `${ctx.remote.user}@${ctx.remote.host}` : ctx.remote.host);
  } else if (ctx.cwd) {
    parts.push(full ? ctx.cwd : (dirName(ctx.cwd) ?? ctx.cwd));
  }
  if (ctx.git) parts.push(ctx.git);
  const prog = programName(ctx.running?.text);
  parts.push(prog ?? (ctx.remote ? (ctx.remote.via ?? "ssh") : "pwsh"));
  return parts.join(" · ");
}

/** 根据命令文本推测程序类型（M5 会用进程树结果覆盖它） */
export function guessKind(command?: string): ProgramKind {
  const p = programName(command)?.toLowerCase();
  if (!p) return "pwsh";
  if (p === "claude") return "claude";
  if (p === "codex") return "codex";
  if (["ssh", "sshp", "sshpass", "mosh", "plink", "telnet"].includes(p)) return "ssh";
  if (["vim", "nvim", "hx", "micro", "nano"].includes(p)) return "editor";
  if (p === "git") return "git";
  if (["node", "npm", "pnpm", "npx", "yarn", "bun", "deno"].includes(p)) return "node";
  if (["python", "python3", "py", "uv", "pip"].includes(p)) return "python";
  if (["cargo", "dotnet", "make", "cmake", "go", "msbuild"].includes(p)) return "build";
  return "other";
}
