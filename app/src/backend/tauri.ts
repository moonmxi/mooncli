// 真正的后端：通过 Tauri 调 Rust 主程序（app/src-tauri/src/session.rs）。
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Backend, CreateOptions, Diagnostics, HistoryHit, SessionMsg } from "./types";

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
    historySearch: (text, opts) =>
      invoke<HistoryHit[]>("history_search", {
        query: { text, nowMs: Date.now(), cwd: opts?.cwd ?? null, limit: opts?.limit ?? 50 },
      }),
    async create(opts: CreateOptions, onMsg: (m: SessionMsg) => void) {
      const channel = new Channel<SessionMsg>();
      channel.onmessage = onMsg;
      await invoke("session_create", { ...opts, channel });
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
