// 补全面板的纯逻辑（docs/04 §4）：把 pwsh、命令说明书、历史三个来源的候选合成一张表，
// 以及选中后要往 pwsh 里发哪些按键。
import { specCandidates } from "./specs";

export type CandidateSource = "pwsh" | "spec" | "history";

export interface Candidate {
  key: string;
  /** 列表里显示的文字 */
  label: string;
  /** 填进去的文字 */
  insert: string;
  desc?: string;
  /** 图标 / 小标签：命令、参数、文件夹、历史…… */
  kind: string;
  source: CandidateSource;
  /** 替换输入行里的哪一段：[start, end)，按字符下标（和 PSReadLine 的光标位置一致，UTF-16） */
  start: number;
  end: number;
}

/** pwsh 的 completions 私有消息（shell/pwsh/MoonCli.ps1 的 F12,F11 处理函数） */
export interface PwshCompletions {
  line: string;
  cursor: number;
  start: number;
  len: number;
  items: { t: string; l?: string; k?: string; d?: string }[];
}

export function parsePwsh(data: unknown): PwshCompletions | null {
  if (typeof data !== "object" || data === null) return null;
  const d = data as Record<string, unknown>;
  if (typeof d.line !== "string" || typeof d.cursor !== "number") return null;
  const items = Array.isArray(d.items) ? d.items.filter((i): i is PwshCompletions["items"][number] => typeof i?.t === "string") : [];
  return { line: d.line, cursor: d.cursor, start: Number(d.start) || 0, len: Number(d.len) || 0, items };
}

/** 光标前的当前词从哪开始（空白分隔；引号里的空格不算分隔） */
export function wordStart(line: string, cursor: number): number {
  let quote: string | null = null;
  let start = 0;
  for (let i = 0; i < cursor; i++) {
    const ch = line[i];
    if (quote) {
      if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (ch === " " || ch === "\t") {
      start = i + 1;
    }
  }
  return start;
}

const PWSH_KIND: Record<string, string> = {
  Command: "命令",
  ParameterName: "参数",
  ParameterValue: "值",
  ProviderContainer: "文件夹",
  ProviderItem: "文件",
  Variable: "变量",
  Property: "属性",
  Method: "方法",
  Type: "类型",
  Keyword: "关键字",
  History: "历史",
  Namespace: "命名空间",
  DynamicKeyword: "关键字",
};

const SPEC_KIND = { command: "命令", subcommand: "子命令", option: "参数" } as const;

export interface MergeInput {
  line: string;
  cursor: number;
  pwsh?: PwshCompletions | null;
  /** 历史库的结果：整条命令 */
  history?: { text: string; count: number }[];
}

/**
 * 合并候选：pwsh 和说明书补"当前词"（说明书带中文说明，同名时说明并到 pwsh 那一项上），
 * 历史补"整行"并排在后面。同样的插入结果只留一个。
 */
export function mergeCandidates({ line, cursor, pwsh, history = [] }: MergeInput, limit = 60): Candidate[] {
  const before = line.slice(0, cursor);
  const ws = wordStart(line, cursor);
  const specs = specCandidates(before);
  const out: Candidate[] = [];
  const seen = new Set<string>();
  const push = (c: Candidate) => {
    const id = `${c.start}:${c.end}:${c.insert}`;
    if (seen.has(id) || !c.insert) return;
    seen.add(id);
    out.push(c);
  };
  // pwsh 的替换范围是它自己算的，最准；行内容对不上（用户在等待期间又打了字）时不用它
  if (pwsh && pwsh.line === line && pwsh.cursor === cursor) {
    const specDesc = new Map(specs.map((s) => [s.text.toLowerCase(), s.desc]));
    for (const it of pwsh.items) {
      push({
        key: `p:${it.t}`,
        label: it.l || it.t,
        insert: it.t,
        desc: specDesc.get(it.t.toLowerCase()) ?? (it.d && it.d !== it.t && it.d !== it.l ? it.d : undefined),
        kind: PWSH_KIND[it.k ?? ""] ?? "补全",
        source: "pwsh",
        start: pwsh.start,
        end: pwsh.start + pwsh.len,
      });
    }
  }
  for (const s of specs) {
    push({ key: `s:${s.text}`, label: s.text, insert: s.text, desc: s.desc, kind: SPEC_KIND[s.kind], source: "spec", start: ws, end: cursor });
  }
  const typed = line.trim();
  for (const h of history) {
    if (h.text === typed) continue;
    push({ key: `h:${h.text}`, label: h.text, insert: h.text, desc: h.count > 1 ? `历史 · ${h.count} 次` : "历史", kind: "历史", source: "history", start: 0, end: line.length });
  }
  return out.slice(0, limit);
}

const LEFT = "\x1b[D";
const RIGHT = "\x1b[C";
const BACKSPACE = "\x7f";
const DELETE = "\x1b[3~";

/**
 * 选中候选后发给 pwsh 的按键：先把光标挪到替换段的末尾，退格删掉这一段，再输入新文字。
 * 比直接改 PSReadLine 的缓冲区简单可靠，也能用 Ctrl+Z 撤销（docs/04 §4）。
 * 替换到行尾的（历史整行）用 Delete 清掉光标后的部分，不按 End——End 会接受 PSReadLine 的灰色联想。
 */
export function keystrokes(line: string, cursor: number, c: Pick<Candidate, "start" | "end" | "insert">): string {
  const start = Math.max(0, Math.min(c.start, line.length));
  const end = Math.max(start, Math.min(c.end, line.length));
  let keys = "";
  if (end === line.length && cursor < end) {
    keys += DELETE.repeat(end - cursor);
    keys += BACKSPACE.repeat(cursor - start);
  } else {
    if (end > cursor) keys += RIGHT.repeat(end - cursor);
    if (end < cursor) keys += LEFT.repeat(cursor - end);
    keys += BACKSPACE.repeat(end - start);
  }
  return keys + c.insert;
}
