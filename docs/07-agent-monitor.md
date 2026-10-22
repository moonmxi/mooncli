# 07 · Agent 用量小窗（Codex）

在 mooncli 里直接看 Codex 的用量：当前会话的上下文占用、账号的 5 小时 / 周额度还剩多少、这一轮跑完没有。不用切窗口，不用敲 `/status`。

> 范围说明：本功能只做 Codex。Claude Code 的同类数据（statusLine `rate_limits` 等）前期调研过，结论存档在本文附录；产品决定先不做，v0.1.1 起代码里只保留 Codex 一条线。

## 1. 合规边界（先说死）

只用两类数据：

1. **Codex 官方扩展点**（notify）送来的内容；
2. **Codex 自己写在本机的文件**（会话 rollout 文件）。

不读任何登录凭证（`~/.codex/auth.json` 碰都不碰）、不调任何未公开网络接口、不注入不拦截。市面上能显示「精确百分比」的第三方工具分两类：一类和我们一样读本地文件；另一类读 OAuth 凭证去调 `chatgpt.com/backend-api/wham/usage` 这类未公开接口——后者一律不做。好在 Codex 的额度快照本来就落盘在会话文件里（见第 2 节），纯本地就能拿到真实百分比，不需要碰红线。

## 2. 数据来源

| 数据 | 来源 |
| --- | --- |
| 模型 | 会话文件里 `turn_context` 的 `model` |
| 上下文占用 | `token_count` 事件的 `info.last_token_usage.input_tokens` ÷ `info.model_context_window` |
| 5 小时额度 | `token_count` 事件带的 `rate_limits.primary`（已用百分比、窗口长度、重置时间） |
| 周额度 | `rate_limits.secondary` |
| 套餐 | `rate_limits.plan_type`（如 plus / pro） |
| 状态（本轮已完成） | notify 的 `agent-turn-complete` 事件（见第 4 节） |

要点（2026-10-22 按 openai/codex 源码核实，细节见附录）：

- rollout 文件在 `~/.codex/sessions/年/月/日/rollout-*.jsonl`，归档在 `archived_sessions/`；每行 `{timestamp, type, payload}`，首行 `session_meta` 带会话 id 与 cwd。`CODEX_HOME` 可改位置（可逗号分隔多个目录，多账号各一个目录时逐个扫、合开展示）。
- `rate_limits` 是客户端把后端响应头解析后落盘的快照，与官方 `/status` 同源——读文件等于读官方数据。账号级额度取最近更新的那个文件里的快照。
- 窗口认 `window_minutes` 动态判断（300 = 5 小时，10080 = 一周）：官方调整过窗口构成（Pro 至今没有 5 小时窗口，primary 缺席是常态），任一窗口都可能为 null，不能按套餐写死。周上限的绝对数值官方从未公布，不许用 token 数反推百分比。
- 会话与标签页的对齐键：文件名尾部 UUID = `session_meta.id` = notify 的 `thread-id`；接入 notify 后能直接关联到 mooncli 的标签页，否则按 cwd 对应，再不行显示最近活动的一个。
- `.jsonl.zst` 压缩归档暂不支持（v1 跳过，不报错）。

## 3. mooncli-bridge

Codex 的 notify 指向的小工具，和主程序放一起发布：

- `mooncli-bridge codex-notify`：读最后一个 argv 的事件 JSON，写进 mooncli 数据目录下的 spool 文件（原子替换写）；用户原来的 notify 若被接入流程保存过，原样转调（不等待）。
- 铁律：永远立刻以 0 退出、不输出任何东西、错误全吞——绝不能因为 mooncli 的问题影响 Codex 本身。
- 它靠环境变量找 mooncli：`MOONCLI_AGENT_DIR`（spool 目录）、`MOONCLI_SESSION_ID`（标签页会话 ID）由 mooncli 在启动标签页时注入。

主进程只读 spool：notify 记录比会话文件新时，把该会话标为「本轮已完成」。notify 已被 Codex 源码标为 legacy（有移除 TODO），所以它只是锦上添花——文件轮询本身是完整兜底，notify 没了功能照常。

## 4. 一键接入（notify）

设置页里一键完成，随时可断开：

- 改的是用户级 `~/.codex/config.toml` **顶层**的 `notify`（写在项目级配置里会被 Codex 忽略）；已有 notify 先存进 mooncli 的 bridge-config，bridge 运行时转调。
- 写入前把原文件备份为同目录下的 `<原名>.mooncli-backup-<时间戳>`；接入前在界面里先预览变更清单，确认才落盘。
- 断开时精确恢复：notify 恢复原样或整行删除，文件其它部分逐字节不动（合并逻辑是纯函数，有单元测试覆盖接入→断开的往返）。

## 5. 小窗

形态：浮动卡片（窗口四角可停靠），跟随当前标签页：

```
Codex · gpt-5-codex                     [plus]
上下文   ██████░░░░  34% · 171k/500k
5 小时   ██░░░░░░░░  27% · 1 小时 42 分
本周     █░░░░░░░░░   9% · 5 天
● 本轮已完成
```

- 显示当前标签页对应会话的上下文 + 账号的额度窗口；多个会话时可展开「全部 N 个」，点任意一个切到它的标签页。
- 进度条颜色随设置里的警告 / 危险阈值变；上下文 ≥85% 时提示可以考虑 /compact。
- 窗口的重置时间已过但还没有新数据时，按「已重置 · 0%」呈现，不显示陈旧百分比（官方在窗口过期后也不再下发旧值，显示旧数会误导）。
- 数据超过 10 分钟没更新时标出快照时间，说明这是本地文件里最近一次记录。
- 命令面板、历史面板等浮层打开时小窗让位；在「Agent 用量」设置节里保留，方便调外观时实时预览。
- 后续形态（未做）：侧栏区块、独立小窗、状态栏一段；浮动卡片的拖动吸附也没做。

## 6. 设置

「设置 → Agent 用量」：

- 总开关（默认关）：关时不采集、不注入环境变量、不显示小窗。
- 两个来源开关（默认开）：读取会话文件 / 接收 notify 状态。关掉前者就只剩接入后的状态提示，后者关掉则不读 spool。
- 小窗外观：停靠角落、不透明度、警告阈值（默认 70%）、危险阈值（默认 90%）。

## 7. 附录：调研结论（2026-10-22，一手来源优先）

**Codex**

- `token_count` 事件结构按 openai/codex 源码（codex-rs/protocol 的 `TokenCountEvent`、`RateLimitSnapshot`）核实：`info` 给 `total/last_token_usage` 与 `model_context_window`；`rate_limits` 的 primary/secondary 各含 `used_percent`（f64）、`window_minutes`、`resets_at`（epoch 秒），另含 `plan_type`。
- notify：用户级 config.toml 顶层 `notify` 数组；唯一真正分发的事件是 `agent-turn-complete`，JSON 以最后一个 argv 传入（含 `thread-id`、`cwd`，无用量数字），通知子进程不被等待；源码在 `legacy_notify.rs`，定性为 legacy。
- 第三方对照：ccusage 等纯本地工具没用 `rate_limits` 字段所以给不出百分比；CodexBar 的额度主路径是读 auth.json 调未公开接口（红线外）；「号池」类工具（CLIProxyAPI 等）可借鉴的只有组织方式，与展示型小窗无关，不引入。

**Claude Code（已调研，未实现）**

- 官方通道是 statusLine 输入的 `rate_limits`（`five_hour`/`seven_day` 的 `used_percentage` + `resets_at` epoch 秒），自 Claude Code **v2.1.80** 加入，仅 Pro/Max 会话、首次 API 响应后出现；`context_window` 字段给上下文占用（`total_input_tokens` 只算输入侧）。
- hooks 载荷里没有任何额度/上下文数字；其余能给百分比的第三方工具靠读 OAuth 凭证调未公开的 `/api/oauth/usage`，在红线外。
- 2026-10-22 产品决定：先只做 Codex，本节仅作存档，将来重启 Claude 支持时从这里接着核实。
