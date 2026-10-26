// 真正的后端：通过 Tauri 调 Rust 主程序（app/src-tauri/src/session.rs）。
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import type {
  AgentAlert,
  Backend,
  CreateOptions,
  DataOverview,
  Diagnostics,
  HistoryDetail,
  HistoryHit,
  HistoryPage,
  HostCount,
  ImportSummary,
  IntegrationOutcome,
  IntegrationStatus,
  SessionMsg,
  SessionStats,
  ShellStatus,
  UsageReport,
} from "./types";

/** 本地时间，用在导出文件名里：20261022-1530 */
function localStamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
}

const log = (what: string) => (e: unknown) => console.error(`mooncli: ${what}失败`, e);

export function tauriBackend(): Backend {
  const win = getCurrentWindow();
  return {
    kind: "tauri",
    loadSettings: () => invoke("settings_get"),
    saveSettings: (value) => invoke("settings_set", { value }),
    clipboardRead: () => invoke<string>("clipboard_read"),
    clipboardWrite: (text) => invoke("clipboard_write", { text }),
    backgroundSave: (fileName, dataBase64) => invoke<string>("background_save", { fileName, dataBase64 }),
    backgroundLoad: (name) => invoke<string>("background_load", { name }),
    backgroundList: () => invoke<string[]>("background_list"),
    backgroundDelete: (name) => invoke("background_delete", { name }),
    diagnostics: () => invoke<Diagnostics>("diagnostics"),
    shellStatus: () => invoke<ShellStatus>("shell_status"),
    openUrl: (url) => invoke("open_url", { url }),
    systemFonts: () => invoke<string[]>("system_fonts"),
    logError(message) {
      invoke("log_error", { message }).catch(() => {});
    },
    logsPack: () => invoke<string>("logs_pack", { label: localStamp() }),
    dataOverview: () => invoke<DataOverview>("data_overview"),
    dataBackupNow: () => invoke<string | null>("data_backup_now"),
    dataRestoreBackup: (name) => invoke("data_restore_backup", { name }),
    settingsReset: () => invoke("settings_reset"),
    // 文件名里用本地时间：mooncli-backup-20261022-1530.zip
    dataExport: (includeHistory) => invoke<string>("data_export", { includeHistory, label: localStamp() }),
    dataImport: (dataBase64, includeHistory) => invoke<ImportSummary>("data_import", { dataBase64, includeHistory }),
    dataReveal: (target) => invoke("data_reveal", { target }),
    historyPrune: (days) => invoke<number>("history_prune", { days }),
    sessionStats: (id) => invoke<SessionStats>("session_stats", { id }),
    agentUsage: (focus) => invoke<UsageReport>("agent_usage", { focus: focus ?? null }),
    agentBridgePath: () => invoke<string>("agent_bridge_path"),
    agentIntegrationStatus: () => invoke<IntegrationStatus>("agent_integration_status"),
    agentRefreshLive: () => invoke("agent_refresh_live"),
    agentWindow: (open) => invoke("agent_window_toggle", { open }),
    agentIntegrationPreview: (target, action) => invoke<IntegrationOutcome>("agent_integration_preview", { target, action }),
    agentIntegrationApply: (target, action) => invoke<IntegrationOutcome>("agent_integration_apply", { target, action }),
    historySearch: (text, opts) =>
      invoke<HistoryHit[]>("history_search", {
        query: { text, nowMs: Date.now(), cwd: opts?.cwd ?? null, limit: opts?.limit ?? 50 },
        lane: opts?.lane ?? null,
      }),
    historyQuery: (q) =>
      invoke<HistoryHit[]>("history_search", {
        query: { dedupe: true, limit: 200, ...q, scope: q.scope ?? { kind: "all" }, nowMs: Date.now() },
        lane: "query",
      }),
    historyPage: (q, lane, offset, limit) =>
      invoke<HistoryPage>("history_page", {
        query: { dedupe: true, ...q, scope: q.scope ?? { kind: "all" }, nowMs: Date.now() },
        lane,
        offset,
        limit,
      }),
    historyDetail: (text) => invoke<HistoryDetail>("history_detail", { text }),
    historyHosts: () => invoke<HostCount[]>("history_hosts"),
    historyFavorite: (text, on) => invoke("history_favorite", { text, on }),
    historyDelete: (text, id) => invoke<number>("history_delete", { text, id: id ?? null }),
    remoteCommand(id, seq, text) {
      invoke("session_remote_command", { id, seq, text }).catch(log("记录远程命令"));
    },
    async create(opts: CreateOptions, onMsg: (m: SessionMsg) => void) {
      const channel = new Channel<SessionMsg>();
      channel.onmessage = onMsg;
      const { launch, ...rest } = opts;
      await invoke("session_create", { ...rest, launchMode: launch ?? null, channel });
    },
    write(id, data) {
      invoke("session_write", { id, data }).catch(log("发送输入"));
    },
    resize(id, cols, rows) {
      invoke("session_resize", { id, cols, rows }).catch(log("改尺寸"));
    },
    ack(id, bytes) {
      invoke("session_ack", { id, bytes }).catch(log("确认输出"));
    },
    close(id) {
      invoke("session_close", { id }).catch(log("关闭会话"));
    },
    onSettingsFile(cb) {
      const offs = [listen("settings-changed", (e) => cb({ value: e.payload })), listen<string>("settings-error", (e) => cb({ error: e.payload }))];
      return () => offs.forEach((p) => p.then((off) => off()));
    },
    attention(title, body, system, tabId) {
      win.requestUserAttention(UserAttentionType.Informational).catch(log("任务栏提醒"));
      if (system) invoke("notify_toast", { title, body, tabId: tabId ?? null }).catch(log("发送系统通知"));
    },
    toast: (title, body) => invoke("notify_toast", { title, body, tabId: null }),
    onAgentAlert(cb) {
      const off = listen<AgentAlert>("agent-alert", (e) => cb(e.payload));
      return () => void off.then((f) => f());
    },
    onToastActivated(cb) {
      const off = listen<string>("toast-activated", (e) => cb(e.payload));
      return () => void off.then((f) => f());
    },
    window: {
      minimize: () => void win.minimize(),
      toggleMaximize: () => void win.toggleMaximize(),
      close: () => void win.close(),
    },
  };
}
