// 终端级快捷键（docs/05 §9）。这些键在任何状态下都由 mooncli 处理，不发给 shell。
// 其它所有按键都原样交给 shell —— 见 docs/05 §1"按键归属"。
//
// 每个操作的快捷键都能在设置里改。按键统一写成 "Ctrl+Shift+T" 这样的字符串，
// 修饰键顺序固定为 Ctrl、Alt、Shift；字母、数字、符号按物理键位识别，不受键盘布局影响。

export type ActionId =
  | "newTab"
  | "closeTab"
  | "duplicateTab"
  | "nextTab"
  | "prevTab"
  | "renameTab"
  | "pinTab"
  | "toggleSidebar"
  | "palette"
  | "history"
  | "settings"
  | "zoomIn"
  | "zoomOut"
  | "zoomReset"
  | "fontBigger"
  | "fontSmaller"
  | "clearScreen"
  | "cycleTheme"
  | "copyCwd"
  | "find"
  | "prevCommand"
  | "nextCommand"
  | "selectOutput"
  | "complete"
  | "splitRight"
  | "splitDown"
  | "closePane"
  | "focusNextPane";

export type Action = { type: ActionId } | { type: "gotoTab"; index: number };

/** 用户改过的快捷键：每个操作一组按键，整组替换默认值；空数组表示不用快捷键 */
export type Keymap = Partial<Record<ActionId, string[]>>;

export interface ActionDef {
  id: ActionId;
  label: string;
  group: "标签页" | "分屏" | "面板" | "命令" | "缩放" | "其它";
  defaults: string[];
  /**
   * 什么时候才由 mooncli 处理：shell = 不在全屏程序里（vim、htop 照样收到这个键）；
   * prompt = 只在本机提示符上（ssh 里、程序运行时原样发给 shell）。不写就是任何时候。
   */
  scope?: "shell" | "prompt";
}

/** 判断快捷键归属时要知道的当前标签页状态 */
export interface KeyContext {
  altScreen?: boolean;
  atPrompt?: boolean;
}

export const ACTIONS: ActionDef[] = [
  { id: "newTab", label: "新建标签页", group: "标签页", defaults: ["Ctrl+Shift+T"] },
  { id: "closeTab", label: "关闭标签页", group: "标签页", defaults: ["Ctrl+Shift+W"] },
  { id: "duplicateTab", label: "复制标签页（同一目录）", group: "标签页", defaults: ["Ctrl+Shift+D"] },
  { id: "nextTab", label: "下一个标签页", group: "标签页", defaults: ["Ctrl+Tab"] },
  { id: "prevTab", label: "上一个标签页", group: "标签页", defaults: ["Ctrl+Shift+Tab"] },
  { id: "renameTab", label: "重命名标签页", group: "标签页", defaults: [] },
  { id: "pinTab", label: "置顶 / 取消置顶标签页", group: "标签页", defaults: [] },
  { id: "toggleSidebar", label: "收起 / 展开标签栏", group: "面板", defaults: ["Ctrl+Shift+B"] },
  { id: "palette", label: "命令面板", group: "面板", defaults: ["Ctrl+Shift+P"] },
  { id: "history", label: "历史命令", group: "面板", defaults: ["Ctrl+Shift+R"] },
  { id: "settings", label: "设置", group: "面板", defaults: ["Ctrl+,"] },
  { id: "zoomIn", label: "放大（整体）", group: "缩放", defaults: ["Ctrl+=", "Ctrl+Shift+=", "Ctrl+Num+"] },
  { id: "zoomOut", label: "缩小（整体）", group: "缩放", defaults: ["Ctrl+-", "Ctrl+Num-"] },
  { id: "zoomReset", label: "恢复默认缩放", group: "缩放", defaults: ["Ctrl+0", "Ctrl+Num0"] },
  { id: "fontBigger", label: "终端字号加大", group: "缩放", defaults: [] },
  { id: "fontSmaller", label: "终端字号减小", group: "缩放", defaults: [] },
  { id: "clearScreen", label: "清除屏幕", group: "其它", defaults: [] },
  { id: "cycleTheme", label: "换一个主题", group: "其它", defaults: [] },
  { id: "copyCwd", label: "复制当前目录路径", group: "其它", defaults: [] },
  { id: "find", label: "在输出中查找", group: "命令", defaults: ["Ctrl+Shift+F"] },
  { id: "prevCommand", label: "跳到上一条命令", group: "命令", defaults: ["Ctrl+Up"], scope: "shell" },
  { id: "nextCommand", label: "跳到下一条命令", group: "命令", defaults: ["Ctrl+Down"], scope: "shell" },
  { id: "selectOutput", label: "选中这条命令的输出", group: "命令", defaults: ["Ctrl+Shift+A"], scope: "shell" },
  // 中文输入法常把 Ctrl+Space 占作中英切换，按键到不了程序，所以再给一个 Alt+/
  { id: "complete", label: "补全面板", group: "命令", defaults: ["Ctrl+Space", "Alt+/"], scope: "prompt" },
  { id: "splitRight", label: "向右分屏", group: "分屏", defaults: ["Ctrl+Shift+E"] },
  { id: "splitDown", label: "向下分屏", group: "分屏", defaults: ["Ctrl+Shift+O"] },
  { id: "closePane", label: "关闭当前窗格", group: "分屏", defaults: [] },
  { id: "focusNextPane", label: "切到下一个窗格", group: "分屏", defaults: ["Alt+`"] },
];

export const actionDef = (id: ActionId) => ACTIONS.find((a) => a.id === id)!;

/** 预设：一键换成某种习惯。覆盖哪些操作就只改哪些，其它保持默认 */
export const PRESETS: { id: string; label: string; desc: string; keys: Keymap }[] = [
  { id: "default", label: "默认", desc: "都用 Ctrl+Shift 组合，不占用 shell 和 vim 里的任何键", keys: {} },
  {
    id: "vscode",
    label: "VS Code / 浏览器习惯",
    desc: "Ctrl+B 收起标签栏、Ctrl+T 新建、Ctrl+W 关闭、Ctrl+R 历史。会挡住 shell 里的这几个键",
    keys: {
      toggleSidebar: ["Ctrl+B"],
      newTab: ["Ctrl+T"],
      closeTab: ["Ctrl+W"],
      history: ["Ctrl+R"],
      palette: ["Ctrl+Shift+P", "F1"],
    },
  },
];

/** 物理键位 → 写法 */
function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Numpad\d$/.test(code)) return `Num${code.slice(6)}`;
  if (/^F\d{1,2}$/.test(code)) return code;
  const map: Record<string, string> = {
    NumpadAdd: "Num+",
    NumpadSubtract: "Num-",
    NumpadMultiply: "Num*",
    NumpadDivide: "Num/",
    NumpadDecimal: "Num.",
    NumpadEnter: "Enter",
    Equal: "=",
    Minus: "-",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Backslash: "\\",
    Semicolon: ";",
    Quote: "'",
    BracketLeft: "[",
    BracketRight: "]",
    Backquote: "`",
    Space: "Space",
    Tab: "Tab",
    Enter: "Enter",
    Escape: "Esc",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
  };
  return map[code] ?? null;
}

type KeyLike = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">;

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock"]);

/** 按键事件 → "Ctrl+Shift+T"。只按了修饰键、或用了 Win 键时返回 null */
export function comboFromEvent(e: KeyLike): string | null {
  if (e.metaKey || MODIFIER_KEYS.has(e.key)) return null;
  // 没有 code 的（测试、少数输入法）退回按字符
  const name = keyName(e.code) ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key === "Tab" ? "Tab" : null);
  if (!name) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
  return [...mods, name].join("+");
}

/** 每个操作现在用的按键 */
export function bindingsOf(id: ActionId, keymap: Keymap = {}): string[] {
  return keymap[id] ?? actionDef(id).defaults;
}

const cache = new WeakMap<Keymap, Map<string, ActionId>>();
const EMPTY: Keymap = {};

function resolve(keymap: Keymap): Map<string, ActionId> {
  let m = cache.get(keymap);
  if (!m) {
    m = new Map();
    for (const a of ACTIONS) for (const k of bindingsOf(a.id, keymap)) if (!m.has(k)) m.set(k, a.id);
    cache.set(keymap, m);
  }
  return m;
}

export function matchShortcut(e: KeyLike, keymap: Keymap = EMPTY, ctx: KeyContext = {}): Action | null {
  const combo = comboFromEvent(e);
  if (!combo) return null;
  const id = resolve(keymap).get(combo);
  if (id) {
    const scope = actionDef(id).scope;
    if (scope === "shell" && ctx.altScreen) return null;
    if (scope === "prompt" && !ctx.atPrompt) return null;
    return { type: id };
  }
  // Alt+1…9 跳到第 N 个标签页（固定，不可改；用户把 Alt+数字 分给别的操作时以用户的为准）
  if (/^Alt\+[1-9]$/.test(combo)) return { type: "gotoTab", index: Number(combo.slice(4)) - 1 };
  return null;
}

export const isAppShortcut = (e: KeyboardEvent, keymap?: Keymap, ctx?: KeyContext) => matchShortcut(e, keymap, ctx) !== null;

/** 界面上显示的快捷键提示：取第一个 */
export function keyHint(id: ActionId, keymap?: Keymap): string | undefined {
  return bindingsOf(id, keymap)[0];
}

/** 已经被哪个操作占用 */
export function ownerOf(combo: string, keymap: Keymap, except?: ActionId): ActionId | undefined {
  for (const a of ACTIONS) if (a.id !== except && bindingsOf(a.id, keymap).includes(combo)) return a.id;
  return undefined;
}

/** 把 combo 设给某个操作（加在它的列表里），同时从别的操作那里拿走。返回新的 keymap 和被拿走的操作 */
export function assign(keymap: Keymap, id: ActionId, combo: string): { keymap: Keymap; takenFrom?: ActionId } {
  const takenFrom = ownerOf(combo, keymap, id);
  const next: Keymap = { ...keymap };
  if (takenFrom) next[takenFrom] = bindingsOf(takenFrom, keymap).filter((k) => k !== combo);
  const mine = bindingsOf(id, keymap);
  if (!mine.includes(combo)) next[id] = [...mine, combo];
  return { keymap: next, takenFrom };
}

export function unassign(keymap: Keymap, id: ActionId, combo: string): Keymap {
  return { ...keymap, [id]: bindingsOf(id, keymap).filter((k) => k !== combo) };
}

export function resetAction(keymap: Keymap, id: ActionId): Keymap {
  const next = { ...keymap };
  delete next[id];
  return next;
}

/** 只保留认识的操作和格式对的按键（读设置文件时用） */
export function cleanKeymap(v: unknown): Keymap {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return {};
  const out: Keymap = {};
  for (const a of ACTIONS) {
    const list = (v as Record<string, unknown>)[a.id];
    if (Array.isArray(list)) out[a.id] = list.filter((k): k is string => typeof k === "string" && k.length > 0 && k.length < 40);
  }
  return out;
}

/** 这些键在 shell、PSReadLine、vim、tmux 里有用处，设成快捷键前提醒一下 */
const SHELL_KEYS: Record<string, string> = {
  "Ctrl+A": "光标移到行首",
  "Ctrl+B": "光标左移一个字（也是 tmux 的前缀键）",
  "Ctrl+C": "中断正在运行的程序",
  "Ctrl+D": "删除字符 / 退出 shell",
  "Ctrl+E": "光标移到行尾",
  "Ctrl+F": "光标右移一个字",
  "Ctrl+H": "退格",
  "Ctrl+K": "删除到行尾",
  "Ctrl+L": "清屏",
  "Ctrl+N": "下一条历史",
  "Ctrl+P": "上一条历史",
  "Ctrl+R": "搜索 shell 自己的历史",
  "Ctrl+S": "向前搜索历史",
  "Ctrl+T": "交换前后两个字",
  "Ctrl+U": "删除到行首",
  "Ctrl+V": "粘贴",
  "Ctrl+W": "删除前一个词",
  "Ctrl+Y": "粘贴刚删掉的内容",
  "Ctrl+Z": "挂起程序 / 撤销",
  "Ctrl+Space": "PSReadLine 的补全菜单",
  "Ctrl+Left": "按词左移",
  "Ctrl+Right": "按词右移",
  "Ctrl+Backspace": "删除前一个词",
  "Alt+B": "按词左移",
  "Alt+F": "按词右移",
  "Alt+D": "删除后一个词",
  Tab: "补全",
  Enter: "执行命令",
  Esc: "vim 等程序要用",
  Up: "上一条历史",
  Down: "下一条历史",
};

/** 设成快捷键会挡住什么。null 表示放心用 */
export function shellConflict(combo: string): string | null {
  if (SHELL_KEYS[combo]) return SHELL_KEYS[combo];
  const parts = combo.split("+");
  const key = parts[parts.length - 1];
  const mods = parts.slice(0, -1);
  if (mods.length === 0 && /^F\d+$/.test(key)) return "htop、mc 等全屏程序会用到功能键";
  if (mods.length === 0) return "单独一个键会让它在终端里打不出来";
  if (mods.length === 1 && (mods[0] === "Ctrl" || mods[0] === "Alt") && /^[A-Z]$/.test(key)) return "shell 或 vim、emacs 里可能会用到";
  return null;
}
