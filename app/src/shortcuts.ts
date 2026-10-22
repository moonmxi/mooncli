// 终端级快捷键（docs/05 §9）。这些键在任何状态下都由 mooncli 处理，不发给 shell。
// 其它所有按键都原样交给 shell —— 见 docs/05 §1"按键归属"。

export type Action =
  | { type: "newTab" }
  | { type: "closeTab" }
  | { type: "nextTab" }
  | { type: "prevTab" }
  | { type: "gotoTab"; index: number }
  | { type: "toggleSidebar" }
  | { type: "find" }
  | { type: "zoom"; dir: 1 | -1 | 0 }
  | { type: "settings" }
  | { type: "palette" }
  | { type: "history" };

export function matchShortcut(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">): Action | null {
  if (e.metaKey) return null;
  const k = e.key.toLowerCase();
  // 整体缩放：按物理键位识别，不受键盘布局影响；小键盘的 + - 0 也算。Ctrl+Shift+= 在美式键盘上就是 Ctrl++
  if (e.ctrlKey && !e.altKey) {
    if (e.code === "Equal" || e.code === "NumpadAdd") return { type: "zoom", dir: 1 };
    if (!e.shiftKey && (e.code === "Minus" || e.code === "NumpadSubtract")) return { type: "zoom", dir: -1 };
    if (!e.shiftKey && (e.code === "Digit0" || e.code === "Numpad0")) return { type: "zoom", dir: 0 };
    if (!e.shiftKey && e.code === "Comma") return { type: "settings" };
  }
  if (e.ctrlKey && e.shiftKey && !e.altKey) {
    if (k === "t") return { type: "newTab" };
    if (k === "w") return { type: "closeTab" };
    if (k === "b") return { type: "toggleSidebar" };
    if (k === "f") return { type: "find" };
    if (k === "p") return { type: "palette" };
    if (k === "r") return { type: "history" };
    if (k === "tab") return { type: "prevTab" };
  }
  if (e.ctrlKey && !e.shiftKey && !e.altKey && k === "tab") return { type: "nextTab" };
  if (e.altKey && !e.ctrlKey && !e.shiftKey && /^Digit[1-9]$/.test(e.code)) {
    return { type: "gotoTab", index: Number(e.code.slice(5)) - 1 };
  }
  return null;
}

export const isAppShortcut = (e: KeyboardEvent) => matchShortcut(e) !== null;
