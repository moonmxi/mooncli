# 07 · Agent 额度与完成提醒（Claude Code、Codex）

在 mooncli 里直接看 Claude Code 和 Codex 的账号额度：5 小时、7 天两个窗口各用了多少、多久后重置。当前标签页正在跑其中一个时，再显示这个会话的上下文占用。一轮跑完或要你确认时，右下角弹系统通知，点一下回到那个标签页。

## 1. 合规边界

只用三类数据：

1. **官方扩展点**送来的内容：Claude Code 的 statusLine 与 hooks、Codex 的 notify；
2. **官方客户端自己的接口**：`codex app-server` 的 `account/rateLimits/read`（Codex 的 IDE 插件用的就是它，登录和联网都由用户本机的 Codex 完成）；
3. **两者自己写在本机的文件**：Claude Code 的会话转录、Codex 的 rollout 文件。

不读任何登录凭证（`~/.codex/auth.json`、Claude Code 的 OAuth 令牌都不碰），mooncli 自己不发网络请求。市面上有些工具读 OAuth 凭证去调 `chatgpt.com/backend-api/wham/usage`、`api.anthropic.com/api/oauth/usage` 这类未公开接口，这类做法一律不用。

## 2. 额度

| | Claude Code | Codex |
| --- | --- | --- |
| 来源 | statusLine 输入里的 `rate_limits.five_hour` / `seven_day`（`used_percentage`、`resets_at`） | `codex app-server` 的 `account/rateLimits/read`；退路是 rollout 文件里 `token_count` 事件带的 `rate_limits` |
| 新鲜度 | Claude Code 每次刷新状态栏都会送来（每轮回复后） | 每两分钟查一次，卡片上可以手动刷新；查一次约 4 秒（起一个 Codex 进程） |
| 前提 | 接入 statusLine（第 5 节）；只有 Pro / Max 订阅、第一次 API 响应之后才有 | 本机装了并登录 Codex；API key 登录没有额度窗口 |

- 额度是账号级的，取所有会话里最新的一份。
- 窗口按长度认（300 分钟 = 5 小时，10080 分钟 = 7 天），不按套餐写死：Codex Pro 没有 5 小时窗口，缺的窗口显示「–」。
- 百分比一律用官方给的值，不用 token 数反推。
- 窗口过了重置时刻还没有新数据时按 0% 显示（官方在窗口过期后也不再下发旧值）。
- 数据超过 10 分钟没更新时，在卡片上标出数据的时刻。

## 3. 当前会话的上下文

只为**当前标签页正在运行的那一个** Agent 找上下文，其它会话一概不看；当前标签页没在跑 Claude Code / Codex 时不显示上下文。界面把「哪个 Agent、哪个标签页、目录、开始运行的时刻」交给后端，后端只看这次运行以来写下的记录。

| | 依次尝试 |
| --- | --- |
| Claude Code | ① 这个标签页送来的 statusLine：`context_window.current_usage` 的输入侧之和、`context_window_size`（上限的权威值）、`used_percentage`；② hooks 记下的转录路径；③ 按标签页目录找 `~/.claude/projects/<目录名>/` 里这次运行以来写过的转录，读最后一条回复的 `usage`。转录对应的会话送过 statusLine 时，仍用 statusLine 的上限 |
| Codex | ① 这个标签页的 notify 带来的 `thread-id`，对上 rollout 文件名；② 这次运行以来写过、`cwd` 和标签页一致的最新 rollout。用最后一个 `token_count` 的 `last_token_usage.total_tokens` 和 `model_context_window`，百分比按 Codex 自己的算法扣掉 12000 的固定开销 |

注意：statusLine 的 `context_window.total_input_tokens` 是整个会话的累计值，不是当前上下文，不能用。

还没有第一轮回复时显示「第一轮回复后显示上下文占用」。

## 4. 完成提醒

| 事件 | 提醒 |
| --- | --- |
| Claude Code `Stop` hook（一轮结束） | 「Claude Code 完成了」，正文：目录 · 最后一条回复的头一句 |
| Claude Code `Notification` hook，`notification_type` 为 `permission_prompt`、`elicitation_dialog` | 「Claude Code 需要你确认」，正文是 Claude Code 给的提示 |
| Codex notify `agent-turn-complete` | 「Codex 完成了」，正文：目录 · `last-assistant-message` 的头一句 |

Claude Code 的空闲提醒（`idle_prompt`，一轮结束后又等了一阵）不再提醒，因为 `Stop` 已经提醒过了。

谁来弹：

- **Agent 跑在 mooncli 的标签页里**（bridge 的祖先进程里有 `mooncli.exe`，且有 `MOONCLI_SESSION_ID`）：bridge 把提醒写进 `agent/alerts/`，主程序半秒内取走交给界面。界面按你在不在看决定怎么提醒：
  - 你正看着这个标签页：不打扰（「正看着时不提醒」可以关）；
  - 窗口在前台但在看别的标签页：标签上亮点，窗口里出一条提示；
  - 窗口不在前台：弹系统通知，点一下拉起 mooncli 并切到那个标签页。
- **跑在别的终端里**（Windows Terminal、VS Code……）：bridge 自己弹系统通知；前台窗口正是运行 Agent 的那个终端时不弹。

系统通知用 WinRT 的 ToastNotification（`tauri-winrt-notification`），身份是 `dev.moonmxi.mooncli`：主程序启动时、bridge 弹通知前都会在 `HKCU\Software\Classes\AppUserModelId\dev.moonmxi.mooncli` 登记名字和图标。没有这一步时，开发版、便携版发出的通知会被 Windows 悄悄丢掉。

## 5. mooncli-bridge 与接入

官方扩展点都指向和主程序放在一起的 `mooncli-bridge.exe`：

| 子命令 | 用在哪 | 做什么 |
| --- | --- | --- |
| `claude-statusline` | Claude Code 的 statusLine | 把输入写进 spool；用户原来的状态栏命令保存过的话原样转调，否则输出一行「模型 · 上下文 · 5 小时 · 本周」 |
| `claude-hook` | `UserPromptSubmit`、`Notification`、`Stop` 三个 hooks | 记下会话最近一个事件（只留会话 ID、事件名、目录、转录路径、通知类型和文字）；后两个事件发提醒 |
| `codex-notify` | Codex 的 `notify` | 记下 `thread-id` 和目录；发提醒；用户原来的 notify 保存过的话原样转调（不等待） |
| `test-toast` | 命令行排查 | 弹一条测试通知 |

铁律：永远以 0 退出，除 statusLine 那一行外不输出东西，错误全吞——不能因为 mooncli 的问题影响 Claude Code、Codex 本身。实测一次 hook 调用 20–50ms（Claude Code 在 Windows 上经 Git Bash 启动 hook 命令本身要 1–3 秒，和 bridge 无关）。

spool 在 `%LOCALAPPDATA%\dev.moonmxi.mooncli\agent\`：`claude-status-<会话>.json`、`claude-hook-<会话>.json`、`codex-notify-<thread>.json`，每个会话每种来源只留最新一份，7 天没更新的自动清掉。用户的提问和 Agent 的回复不落进这个目录。

**一键接入**（设置 → Agent）：

- Claude Code：改 `~/.claude/settings.json`。原 statusLine 整个对象存进 `bridge-config.json` 由 bridge 转调；hooks 并列追加，不覆盖用户已有的同事件 hooks。
- Codex：只改用户级 `~/.codex/config.toml` **顶层**的 `notify`（项目级的会被 Codex 忽略）；原 notify 存进 bridge-config 由 bridge 转调。
- 写入前在界面里预览变更清单，确认才落盘；改前把原文件备份为同目录下的 `<原名>.mooncli-backup-<时间戳>`。
- 断开时精确恢复：文件没被别人改过就逐字节放回原文；改过的话只删 mooncli 加的部分，其它内容不动（合并逻辑是纯函数，有接入→断开的往返测试）。
- 不想让 mooncli 改文件，设置页里有「自己手动配置」，给出要写的内容和复制按钮。

## 6. 卡片

```
Agent 额度                                  刷新
───────────────────────────────────────────────
Claude Code
5 小时   ██████░░░░   46%  2 小时 18 分
7 天     █░░░░░░░░░    6%  4 天
───────────────────────────────────────────────
Codex  plus
5 小时   ░░░░░░░░░░    1%  3 小时 9 分
7 天     ░░░░░░░░░░    0%  7 天
───────────────────────────────────────────────
当前会话 · Claude Code · Opus
上下文   █░░░░░░░░░   13%  126k/1M
```

- 形态：浮动卡片（停在窗口一角，按住标题拖到别的角）、状态栏一段（`Claude 46% · 6%   Codex 1% · 0%`，点击收起或展开卡片）、独立置顶小窗（切到别的程序时也能看到；它没有标签页，不显示上下文）。
- 缺数据时直说原因：Claude Code 没接入、还没有下一次回复、Codex 查询失败的原因。
- 进度条颜色随警告 / 危险阈值变（默认 70% / 90%）；上下文 ≥85% 时提示可以 /compact。
- 命令面板、历史面板等浮层打开时卡片让位；在「设置 → Agent」里保留，方便调外观时实时预览。

## 7. 设置

「设置 → Agent」：

- 接入 Claude Code / Codex（一键或手动）。
- 完成提醒：开关（默认开）、正看着时不提醒（默认开）、发送测试通知。
- 额度卡片：开关（默认关）、浮动卡片显示与否、状态栏摘要、独立小窗、Codex 实时额度（默认开）、codex 位置。
- 卡片外观：停靠角落、不透明度、警告阈值、危险阈值。

## 8. 附录：核实记录（2026-10-22）

- **Claude Code statusLine**（code.claude.com/docs/en/statusline，Claude Code 2.1.302 实测）：`rate_limits.five_hour` / `seven_day` 各带 `used_percentage`、`resets_at`（epoch 秒）；`context_window` 带 `current_usage`（本次请求的 input / cache_creation / cache_read）、`context_window_size`、`used_percentage`，以及会话累计的 `total_input_tokens`。用 `claude --settings <临时文件>` 挂上 bridge 实测，拿到的值和 Claude Code 自己状态栏显示的一致。
- **Claude Code hooks**：`Stop`、`Notification`（带 `notification_type`、`message`）、`UserPromptSubmit` 的输入都带 `session_id`、`cwd`、`transcript_path`；载荷里没有额度和上下文数字。
- **Codex app-server**（`codex app-server generate-json-schema` 导出的 v2 协议，codex-cli 0.144.6 实测）：`initialize` → `initialized` → `account/rateLimits/read`，结果的 `rateLimitsByLimitId.codex`（或兼容的 `rateLimits`）带 `primary` / `secondary` 的 `usedPercent`、`windowDurationMins`、`resetsAt`，以及 `planType`。
- **Codex rollout**（openai/codex 源码 `TokenCountEvent`、`RateLimitSnapshot`）：`~/.codex/sessions/年/月/日/rollout-<时间>-<thread id>.jsonl`，首行 `session_meta` 带 `id`、`cwd`；`token_count` 事件的 `info` 给 `last_token_usage` 和 `model_context_window`，`rate_limits` 是后端下发后落盘的快照。`CODEX_HOME` 可改位置（可逗号分隔多个）。
- **Codex notify**：用户级 config.toml 顶层的 `notify` 数组，事件 JSON 作为最后一个参数传入；交互模式和 `codex exec` 都会在一轮结束时发 `agent-turn-complete`（带 `thread-id`、`cwd`、`last-assistant-message`），不等待子进程。源码里定性为 legacy，所以额度不依赖它，它只负责提醒和关联标签页。
