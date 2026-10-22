// 不想让 mooncli 改配置文件时，自己手动接入要写的内容（和 crates/mooncli-agent/src/integrate.rs 写的一致）。

/** Claude Code / Codex 在 Windows 上都认正斜杠路径（Claude Code 用 Git Bash 跑命令，反斜杠会被当成转义） */
const slash = (p: string) => p.replace(/\\/g, "/");

const HOOK_EVENTS = ["UserPromptSubmit", "Notification", "Stop"] as const;

/** 合并进 ~/.claude/settings.json 的内容（已有 hooks 的话把这几条追加进对应事件的数组） */
export function claudeSnippet(bridgePath: string): string {
  const cmd = (sub: string) => `"${slash(bridgePath)}" ${sub}`;
  const hook = [{ hooks: [{ type: "command", command: cmd("claude-hook") }] }];
  return JSON.stringify(
    {
      statusLine: { type: "command", command: cmd("claude-statusline") },
      hooks: Object.fromEntries(HOOK_EVENTS.map((e) => [e, hook])),
    },
    null,
    2,
  );
}

/** 写在 ~/.codex/config.toml 顶层（第一个 [表] 之前）的一行；单引号是 TOML 的字面量字符串，路径不用转义 */
export function codexSnippet(bridgePath: string): string {
  return `notify = ['${slash(bridgePath)}', 'codex-notify']`;
}
