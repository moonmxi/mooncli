import { demoBackend } from "./demo";
import { tauriBackend } from "./tauri";
import type { Backend } from "./types";

/** 在 Tauri 窗口里用真后端，直接用浏览器打开时用演示后端。 */
export function pickBackend(): Backend {
  return "__TAURI_INTERNALS__" in window ? tauriBackend() : demoBackend();
}
