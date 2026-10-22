// settings.json 的 JSON Schema（schema/settings.schema.json）：从默认设置推出来，
// 编辑器（VS Code 等）打开 settings.json 时就有补全、说明和类型检查。
// schema.test.ts 检查仓库里的文件和这里生成的一致，改了设置结构记得跑 `UPDATE_SCHEMA=1 pnpm test` 更新。
import { ACTIONS } from "../shortcuts";
import { DEFAULTS } from "./settings";

type Json = Record<string, unknown>;

/** 有固定取值的字段 */
const ENUMS: Record<string, string[]> = {
  "background.fit": ["cover", "contain", "tile", "center"],
  "background.scope": ["terminal", "window"],
  "tabs.position": ["left", "top"],
  "terminal.cursorStyle": ["bar", "block", "underline"],
  "terminal.rightClick": ["copyPaste", "menu"],
  "agent.widget.style": ["floating", "off"],
  "agent.widget.anchor": ["bottom-right", "bottom-left", "top-right", "top-left"],
};

/** 几个关键字段的说明（其余字段的含义见 docs/03 §6） */
const DESCRIPTIONS: Record<string, string> = {
  theme: "主题名（moon-paper / frost / sakura-dawn / celadon / night-voyage / 导入的 custom-*），或 {light, dark, follow} 跟随系统深浅色",
  zoom: "整体缩放：界面和终端一起",
  uiScale: "只缩放标题栏、侧栏、状态栏、面板",
  "font.family": "终端字体回退链，逗号分隔",
  "background.image": "null 不用背景图；builtin:xxx 内置；其它是导入的文件名",
  "shell.path": "pwsh 的位置，null 时自动查找",
  "shell.integration": "shell 集成（提示符、命令、目录标记）",
  "history.keepDays": "只保留最近多少天的历史，0 表示一直保留",
  "remote.connectCommands": "也当作「连接服务器」的命令（写法和 ssh 一样）",
  agent: "Agent 额度卡片与完成提醒（Claude Code 与 Codex），docs/07",
  "agent.liveQuota": "通过本机 codex app-server 实时查询 Codex 额度",
  "tabs.restoreSession": "打开时恢复上次的标签页",
  notifications: "后台标签页需要关注、窗口又不在前台时的提醒",
  customThemes: "导入的配色（结构见 theme.schema.json）",
  keys: "改过的快捷键：操作 → 按键列表，比如 \"newTab\": [\"Ctrl+T\"]",
};

function node(value: unknown, path: string): Json {
  const out: Json = {};
  if (DESCRIPTIONS[path]) out.description = DESCRIPTIONS[path];
  if (ENUMS[path]) return { ...out, enum: ENUMS[path], default: value };
  if (value === null) return { ...out, type: ["string", "null"], default: null };
  if (Array.isArray(value)) return { ...out, type: "array", default: value };
  switch (typeof value) {
    case "number":
      return { ...out, type: "number", default: value };
    case "boolean":
      return { ...out, type: "boolean", default: value };
    case "string":
      return { ...out, type: "string", default: value };
    case "object": {
      const properties: Json = {};
      for (const [k, v] of Object.entries(value as Json)) properties[k] = node(v, path ? `${path}.${k}` : k);
      return { ...out, type: "object", properties };
    }
    default:
      return out;
  }
}

export function settingsSchema(): Json {
  const root = node(DEFAULTS, "") as { properties: Json };
  const props = root.properties as Record<string, Json>;
  props.theme = {
    description: DESCRIPTIONS.theme,
    oneOf: [
      { type: "string" },
      {
        type: "object",
        properties: { light: { type: "string" }, dark: { type: "string" }, follow: { enum: ["system", "off"] } },
        required: ["light", "dark", "follow"],
      },
    ],
    default: DEFAULTS.theme,
  };
  props.customThemes = { description: DESCRIPTIONS.customThemes, type: "array", items: { $ref: "theme.schema.json" }, default: [] };
  props.keys = {
    description: DESCRIPTIONS.keys,
    type: "object",
    properties: Object.fromEntries(ACTIONS.map((a) => [a.id, { description: a.label, type: "array", items: { type: "string" }, default: a.defaults }])),
    additionalProperties: false,
    default: {},
  };
  return {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $id: "https://raw.githubusercontent.com/moonmxi/mooncli/main/schema/settings.schema.json",
    title: "mooncli 设置",
    description: "mooncli 的 settings.json。设置页里的改动都会写到这里；直接编辑也会立即生效。",
    ...root,
  };
}
