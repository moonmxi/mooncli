// 会话消息：Rust 主程序（或浏览器里的演示后端）发给界面的内容。
// 和 app/src-tauri/src/session.rs 里的 UiMsg 一一对应。

export type ShellState = "unknown" | "prompt" | "running" | "altScreen";

export type SessionMsg =
  | { type: "output"; data: string; bytes: number }
  | { type: "state"; state: ShellState }
  | { type: "commandStarted"; text: string | null }
  | { type: "commandFinished"; text: string | null; exitCode: number | null }
  | { type: "remotePrompt" }
  | { type: "remoteConnecting"; target: ConnectTarget }
  | { type: "remoteConnected"; target: ConnectTarget }
  /** `text` 由界面从屏幕上读出来后补上（TerminalView），Rust 发来时没有 */
  | { type: "remoteCommandStarted"; seq: number; text?: string }
  | { type: "remoteCommandFinished"; seq: number; exitCode: number | null }
  | { type: "remoteDisconnected"; exitCode: number | null }
  | { type: "cwd"; host: string | null; path: string }
  | { type: "title"; text: string; activity: string | null; informative: boolean }
  | { type: "bell" }
  | { type: "notify"; title: string | null; body: string }
  | { type: "progress"; state: number; value: number }
  | { type: "env"; pwsh?: string; host?: string; admin?: boolean }
  | { type: "completions"; data: unknown }
  | { type: "process"; kind: ProgramKind; exe: string; pid: number } // M5：进程探测
  | { type: "exited"; code: number | null };

/** 连接命令要连去哪（crates/mooncli-protocol/src/remote.rs） */
export interface ConnectTarget {
  via: string;
  host: string;
  user: string | null;
}

export type ProgramKind = "pwsh" | "claude" | "codex" | "ssh" | "editor" | "git" | "node" | "python" | "build" | "other";

export interface CreateOptions {
  id: string;
  cols: number;
  rows: number;
  cwd?: string;
}

export interface Diagnostics {
  appVersion: string;
  os: string;
  osVersion: string;
  shellPath: string | null;
  shellKind: string | null;
  integrationEnabled: boolean;
  bundledConpty: boolean;
  settingsPath: string;
  historyPath: string;
  historyCount: number | null;
  backgroundsDir: string;
}

export interface SessionStats {
  outputBytes: number;
  trustedMarks: number;
  untrustedMarks: number;
  privateMessages: number;
  ageMs: number;
}

export interface HistoryHit {
  id: number;
  text: string;
  count: number;
  lastUsed: number;
  lastCwd: string | null;
  lastExit: number | null;
  durationMs: number | null;
  host: string | null;
  favorite: boolean;
  matched: number[];
}

export type HistoryScope = { kind: "all" } | { kind: "cwd"; value: string } | { kind: "session"; value: string };

export interface HistoryQuery {
  text: string;
  scope?: HistoryScope;
  successOnly?: boolean;
  failedOnly?: boolean;
  favoritesOnly?: boolean;
  host?: string | null;
  dedupe?: boolean;
  sort?: "relevance" | "recent";
  limit?: number;
  cwd?: string | null;
  sessionId?: string | null;
}

export interface HistoryRun {
  id: number;
  startedAt: number;
  cwd: string | null;
  exitCode: number | null;
  durationMs: number | null;
  host: string | null;
  shell: string | null;
  imported: boolean;
}

export interface HistoryDetail {
  summary: { count: number; ok: number; failed: number; firstUsed: number | null; avgDurationMs: number | null; dirs: number };
  runs: HistoryRun[];
}

export interface HostCount {
  host: string;
  count: number;
  remote: boolean;
}

export interface Backend {
  readonly kind: "tauri" | "demo";
  loadSettings(): Promise<unknown>;
  saveSettings(value: unknown): Promise<void>;
  clipboardRead(): Promise<string>;
  clipboardWrite(text: string): Promise<void>;
  backgroundSave(fileName: string, dataBase64: string): Promise<string>;
  backgroundLoad(name: string): Promise<string>;
  backgroundList(): Promise<string[]>;
  backgroundDelete(name: string): Promise<void>;
  diagnostics(): Promise<Diagnostics>;
  sessionStats(id: string): Promise<SessionStats>;
  historySearch(text: string, opts?: { cwd?: string; limit?: number }): Promise<HistoryHit[]>;
  /** 历史面板用的完整查询 */
  historyQuery(q: HistoryQuery): Promise<HistoryHit[]>;
  historyDetail(text: string): Promise<HistoryDetail>;
  historyHosts(): Promise<HostCount[]>;
  historyFavorite(text: string, on: boolean): Promise<void>;
  /** 给了 id 只删那一次，否则删这条命令的全部记录 */
  historyDelete(text: string, id?: number): Promise<number>;
  /** 界面读出的第 seq 条远程命令的文字 */
  remoteCommand(id: string, seq: number, text: string): void;
  create(opts: CreateOptions, onMsg: (m: SessionMsg) => void): Promise<void>;
  write(id: string, data: string): void;
  resize(id: string, cols: number, rows: number): void;
  /** 界面处理完这么多字节的输出（背压，docs/02 §2） */
  ack(id: string, bytes: number): void;
  close(id: string): void;
  window: {
    minimize(): void;
    toggleMaximize(): void;
    close(): void;
  };
}
