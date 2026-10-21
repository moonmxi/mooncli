// 终端级快捷键（docs/05 §9）。这些键在任何状态下都由 mooncli 处理，不发给 shell。
// 其它所有按键都原样交给 shell —— 见 docs/05 §1"按键归属"。

export type Action =
  | { type: "newTab" }
  | { type: "closeTab" }
  | { type: "nextTab" }
  | { type: "prevTab" }
  | { type: "gotoTab"; index: number }
  | { type: "toggleSidebar" }
  | { type: "find" };

export function matchShortcut(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">): Action | null {
  if (e.metaKey) return null;
  const k = e.key.toLowerCase();
  if (e.ctrlKey && e.shiftKey && !e.altKey) {
    if (k === "t") return { type: "newTab" };
    if (k === "w") return { type: "closeTab" };
    if (k === "b") return { type: "toggleSidebar" };
    if (k === "f") return { type: "find" };
    if (k === "tab") return { type: "prevTab" };
  }
  if (e.ctrlKey && !e.shiftKey && !e.altKey && k === "tab") return { type: "nextTab" };
  if (e.altKey && !e.ctrlKey && !e.shiftKey && /^Digit[1-9]$/.test(e.code)) {
    return { type: "gotoTab", index: Number(e.code.slice(5)) - 1 };
  }
  return null;
}

export const isAppShortcut = (e: KeyboardEvent) => matchShortcut(e) !== null;
