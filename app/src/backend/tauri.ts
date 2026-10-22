// 真正的后端：通过 Tauri 调 Rust 主程序（app/src-tauri/src/session.rs）。
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  Backend,
  CreateOptions,
  DataOverview,
  Diagnostics,
  HistoryDetail,
  HistoryHit,
  HostCount,
  ImportSummary,
  SessionMsg,
  SessionStats,
  ShellStatus,
} from "./types";

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
    dataOverview: () => invoke<DataOverview>("data_overview"),
    dataBackupNow: () => invoke<string | null>("data_backup_now"),
    dataRestoreBackup: (name) => invoke("data_restore_backup", { name }),
    settingsReset: () => invoke("settings_reset"),
    dataExport: (includeHistory) => {
      // 文件名里用本地时间：mooncli-backup-20261022-1530.zip
      const d = new Date();
      const p = (n: number) => String(n).padStart(2, "0");
      const label = `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
      return invoke<string>("data_export", { includeHistory, label });
    },
    dataImport: (dataBase64, includeHistory) => invoke<ImportSummary>("data_import", { dataBase64, includeHistory }),
    dataReveal: (target) => invoke("data_reveal", { target }),
    historyPrune: (days) => invoke<number>("history_prune", { days }),
    sessionStats: (id) => invoke<SessionStats>("session_stats", { id }),
    historySearch: (text, opts) =>
      invoke<HistoryHit[]>("history_search", {
        query: { text, nowMs: Date.now(), cwd: opts?.cwd ?? null, limit: opts?.limit ?? 50 },
      }),
    historyQuery: (q) =>
      invoke<HistoryHit[]>("history_search", {
        query: { dedupe: true, limit: 200, ...q, scope: q.scope ?? { kind: "all" }, nowMs: Date.now() },
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
    window: {
      minimize: () => void win.minimize(),
      toggleMaximize: () => void win.toggleMaximize(),
      close: () => void win.close(),
    },
  };
}
