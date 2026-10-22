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
  /** 进程树探测：pwsh 底下正在跑的进程链（exe 名，小写），空表示没有子进程 */
  | { type: "process"; chain: string[]; pid: number | null }
  | { type: "git"; branch: string | null }
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
  /** 不填是 pwsh 7（state/store.ts 的 LaunchMode） */
  launch?: "pwsh" | "windowsPowerShell" | "installPwsh";
}

/** 找不到 pwsh 7 时 create 报的错以它开头 */
export const NO_PWSH = "NO_PWSH";

export interface DataOverview {
  configDir: string;
  dataDir: string;
  exportDir: string;
  settingsBytes: number;
  backgrounds: { name: string; bytes: number }[];
  backups: { name: string; at: number; bytes: number; reason: string }[];
  backupsBytes: number;
  historyBytes: number;
  historyCount: number | null;
}

export interface ImportSummary {
  /** 导入后的设置（备份里没有设置时为 null） */
  settings: unknown | null;
  backgrounds: number;
  historyAdded: number | null;
}

export interface ShellStatus {
  pwsh: string | null;
  /** 设置里指定了路径，但那里没有文件 */
  configuredMissing: boolean;
  windowsPowershell: string | null;
  winget: boolean;
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

// ---- Agent 用量（crates/mooncli-agent 的 UsageReport，一一对应） ----

export type AgentKind = "claude" | "codex";

/** 数据从哪来：statusLine = Claude Code 官方扩展点；live = codex app-server 实时查询；sessionFile = 会话文件里的快照 */
export type SnapshotSource = "statusLine" | "live" | "sessionFile";

export interface RateWindow {
  /** 已用百分比 0–100（官方数据，不自行推算） */
  usedPercent: number;
  windowMinutes?: number;
  /** 重置时刻（Unix epoch 秒） */
  resetsAt?: number;
  estimated: boolean;
}

export interface AgentContextUsage {
  usedTokens: number;
  windowTokens: number;
}

export type AgentSessionState = "running" | "waiting" | "done" | "unknown";

export interface AgentSessionUsage {
  kind: AgentKind;
  sessionId: string;
  tabSessionId?: string;
  cwd?: string;
  model?: string;
  context?: AgentContextUsage;
  state: AgentSessionState;
  /** 快照时刻（epoch 秒） */
  updatedAt: number;
  source: SnapshotSource;
}

export interface AgentAccountUsage {
  kind: AgentKind;
  primary?: RateWindow;
  secondary?: RateWindow;
  plan?: string;
  updatedAt: number;
  source: SnapshotSource;
}

export interface UsageReport {
  sessions: AgentSessionUsage[];
  accounts: AgentAccountUsage[];
  /** 实时额度最近一次查询失败的原因 */
  liveError?: string | null;
}

export interface IntegrationStatus {
  claude: boolean;
  codex: boolean;
  claudeSettingsPath: string;
  codexConfigPath: string;
  /** 找到的 codex 可执行文件（实时额度要用） */
  codexExe: string | null;
  bridgeExists: boolean;
}

export interface IntegrationOutcome {
  changes: string[];
}

export type IntegrationTarget = "claude" | "codex";
export type IntegrationAction = "integrate" | "unintegrate";

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
  /** 重新找一遍 shell（刚装好 pwsh 7 时用） */
  shellStatus(): Promise<ShellStatus>;
  /** 用系统浏览器打开 https 网址 */
  openUrl(url: string): Promise<void>;
  // ---- 数据管理（app/src-tauri/src/data.rs） ----
  dataOverview(): Promise<DataOverview>;
  dataBackupNow(): Promise<string | null>;
  /** 恢复某份设置备份，返回恢复后的设置 */
  dataRestoreBackup(name: string): Promise<unknown>;
  /** 恢复默认设置（先自动备份），返回新的设置 */
  settingsReset(): Promise<unknown>;
  /** 导出备份包，返回文件路径 */
  dataExport(includeHistory: boolean): Promise<string>;
  dataImport(dataBase64: string, includeHistory: boolean): Promise<ImportSummary>;
  /** 在文件管理器里打开："config" | "data" | "export" | 导出的备份文件路径 */
  dataReveal(target: string): Promise<void>;
  /** 删掉 days 天前的命令历史；0 表示全部删掉。返回删掉的条数 */
  historyPrune(days: number): Promise<number>;
  sessionStats(id: string): Promise<SessionStats>;
  /** Agent 用量小窗的数据（docs/07，Claude Code 与 Codex）；功能没开时后端返回空报告 */
  agentUsage(): Promise<UsageReport>;
  /** 立刻通过 codex app-server 重新查一次实时额度 */
  agentRefreshLive(): Promise<void>;
  /** 打开 / 关闭独立的置顶用量小窗 */
  agentWindow(open: boolean): Promise<void>;
  /** mooncli-bridge 的路径，设置页「一键接入」展示用 */
  agentBridgePath(): Promise<string>;
  agentIntegrationStatus(): Promise<IntegrationStatus>;
  /** 预览接入/断开会改什么（不落盘） */
  agentIntegrationPreview(target: IntegrationTarget, action: IntegrationAction): Promise<IntegrationOutcome>;
  /** 执行接入/断开（改前自动备份原文件） */
  agentIntegrationApply(target: IntegrationTarget, action: IntegrationAction): Promise<IntegrationOutcome>;
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
  /** 设置文件被外部修改（热更新）；error 是写坏时的出错信息。返回取消订阅函数 */
  onSettingsFile(cb: (e: { value?: unknown; error?: string }) => void): () => void;
  /** 提醒用户：任务栏图标闪烁；system 为真时再发一条系统通知 */
  attention(title: string, body: string, system: boolean): void;
  window: {
    minimize(): void;
    toggleMaximize(): void;
    close(): void;
  };
}
