// 会话消息：Rust 主程序（或浏览器里的演示后端）发给界面的内容。
// 和 app/src-tauri/src/session.rs 里的 UiMsg 一一对应。

export type ShellState = "unknown" | "prompt" | "running" | "altScreen";

export type SessionMsg =
  | { type: "output"; data: string; bytes: number }
  | { type: "state"; state: ShellState }
  | { type: "commandStarted"; text: string | null }
  | { type: "commandFinished"; text: string | null; exitCode: number | null }
  | { type: "remotePrompt" }
  | { type: "cwd"; host: string | null; path: string }
  | { type: "title"; text: string; activity: string | null; informative: boolean }
  | { type: "bell" }
  | { type: "notify"; title: string | null; body: string }
  | { type: "progress"; state: number; value: number }
  | { type: "env"; pwsh?: string; host?: string; admin?: boolean }
  | { type: "completions"; data: unknown }
  | { type: "process"; kind: ProgramKind; exe: string; pid: number } // M5：进程探测
  | { type: "exited"; code: number | null };

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
  shellPath: string | null;
  shellKind: string | null;
  integrationEnabled: boolean;
  bundledConpty: boolean;
  settingsPath: string;
  historyPath: string;
  historyCount: number | null;
  backgroundsDir: string;
}

export interface HistoryHit {
  id: number;
  text: string;
  count: number;
  lastUsed: number;
  lastCwd: string | null;
  lastExit: number | null;
  matched: number[];
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
  historySearch(text: string, opts?: { cwd?: string; limit?: number }): Promise<HistoryHit[]>;
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
