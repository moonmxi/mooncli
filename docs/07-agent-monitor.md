# 07 Agent 用量小窗

目标：用 Claude Code 和 Codex 时，不用敲 `/usage`、`/status`，随时能看到**5 小时额度、周额度、上下文占用**，并且做法合规。

## 1. 合规边界

**只用两类数据：**

1. **工具官方提供的扩展点**：Claude Code 的 statusLine（自定义状态栏）和 hooks（钩子），Codex 的 `notify` 配置。这些就是设计给第三方程序接的。
2. **工具自己写在本机的文件**：会话记录、日志等。这些文件属于用户自己，读它们和用户自己打开看一样。

**明确不做：**

- 不读取、不复制、不使用任何登录凭证（token、cookie、`credentials.json` 之类）。
- 不调用任何未公开的网络接口，不模拟客户端请求去"查额度"。
- 不修改 Agent 的程序文件，不注入代码，不拦截它们的网络流量。
- 不把任何数据发出本机。

做不到精确的数据时，宁可显示"估算"或"不可用"，也不越过上面的边界。

## 2. 各项数据从哪来

> 下表里 Claude Code 和 Codex 的具体字段会随版本变化，**实现前要按当时的官方文档和实际文件逐个核实**，并在代码里做好"字段不存在"的降级。

| 数据 | Claude Code | Codex |
| --- | --- | --- |
| 模型 | statusLine 输入的 `model.display_name` | 会话文件里 `turn_context` 的 `model` |
| 上下文占用 | ① statusLine 输入的 `context_window`（官方口径：`total_input_tokens` = 输入+缓存读+缓存写，不含输出；`context_window_size` 是上限权威值）✅ 已核实；② 否则读 `transcript_path` 指向的会话记录，取最后一条回复的 token 用量（输入 + 缓存读取 + 缓存写入）÷ 模型上下文上限 | 会话文件里 `token_count` 事件的 `info.last_token_usage.input_tokens` ÷ `info.model_context_window` ✅ 已核实 |
| 5 小时额度 | ① statusLine 输入的 `rate_limits.five_hour`（`used_percentage`、`resets_at` epoch 秒；Claude Code v2.1.80 起官方提供，仅 Pro/Max 订阅、首次 API 响应后出现）✅ 已核实；② 否则为"估算"：汇总本机所有会话记录在当前 5 小时窗口内的用量（社区工具 ccusage 用的就是这个方法），显示用量和窗口重置时间，百分比需要用户校准（见下） | 会话文件里 `token_count` 事件带的 `rate_limits.primary`（已用百分比、窗口长度、重置时间）✅ 已核实 |
| 周额度 | 同上（`rate_limits.seven_day`） | `rate_limits.secondary` ✅ 已核实 |
| 本次花费 | statusLine 输入的 `cost.total_cost_usd`（API 计费用户才有意义） | 不显示 |
| 状态（运行/等待/完成） | hooks：`Notification`、`Stop`、`SessionStart`、`SessionEnd` 等 | `notify` 配置（一轮结束时调用指定程序） |

> 核实细节与来源见文末附录（2026-10-22）。Codex 的窗口必须认 `window_minutes` 动态判断：Pro 套餐目前没有 5 小时窗口（primary 缺席是常态），任一窗口都可能为 null。

### 估算值的校准

Claude Code 的额度百分比如果拿不到官方数据，本机只能算出"用了多少 token"，不知道上限是多少。做法：

- 小窗上显示"约 1.2M tokens · 3 小时 12 分后重置"，不显示百分比。
- 用户在 Claude Code 里敲 `/usage` 看到真实百分比后，可以在小窗上点"校准"填进去，mooncli 由此反推上限，之后显示估算百分比，并一直带"≈"标记。
- 上限会随套餐和官方策略变，校准值 7 天后变灰提醒重新校准。

## 3. 怎么接上 Claude Code

### statusLine

Claude Code 允许在 `settings.json` 里配置一个命令作为状态栏，每次刷新时把当前会话信息以 JSON 从标准输入传给它，命令的输出就显示在 Claude Code 底部。

mooncli 配置成：

```jsonc
// ~/.claude/settings.json
{
  "statusLine": {
    "type": "command",
    "command": "mooncli-bridge statusline"
  }
}
```

`mooncli-bridge statusline` 做三件事：

1. 读标准输入的 JSON，连同环境变量 `MOONCLI_SESSION_ID` 一起写进 mooncli 数据目录下的 spool 文件（原子替换写；写不进去就算了，不阻塞）。（原设计是命名管道，实现时改为 spool 文件：更简单、跨重启不丢状态，效果等价。）
2. **如果用户原来就有自己的状态栏命令**：mooncli 在安装时把原命令保存起来，bridge 原样调用它、把输入转给它、把它的输出打印出来。用户原来的状态栏照常显示，一点不受影响。
3. 没有原命令时，可选输出 mooncli 自带的一行简洁状态栏（模型 · 上下文 · 目录），也可以什么都不输出。

不在 mooncli 里运行的 Claude Code（比如在 VS Code 终端里）没有 `MOONCLI_SESSION_ID`，bridge 只做第 2、3 步，不上报。

### hooks

```jsonc
{
  "hooks": {
    "Notification": [{ "hooks": [{ "type": "command", "command": "mooncli-bridge hook notification" }] }],
    "Stop":         [{ "hooks": [{ "type": "command", "command": "mooncli-bridge hook stop" }] }],
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "mooncli-bridge hook session-start" }] }],
    "SessionEnd":   [{ "hooks": [{ "type": "command", "command": "mooncli-bridge hook session-end" }] }]
  }
}
```

- 钩子命令必须很快、永不失败：bridge 总是立刻以 0 退出，不输出任何内容，不影响 Claude Code 的行为。
- 和用户已有的 hooks **并列追加**，不覆盖。

### 一键接入

设置 → Agent → "接入 Claude Code"：

1. 显示将要做的修改（diff 形式），用户确认后才写入。
2. 修改前备份原文件到 `~/.claude/settings.json.mooncli-backup-<时间>`。
3. 提供"断开"按钮，精确撤销 mooncli 加的部分，还原原状态栏命令。
4. 可选择写入用户级（`~/.claude/settings.json`）还是只写某个项目。

## 4. 怎么接上 Codex

- **用量**：`mooncli-agent` 读 `~/.codex/sessions/`（及 `archived_sessions/`）的 rollout 文件尾部（路径与格式已按 openai/codex 源码核实，见附录），有新内容时只读尾部增量，解析其中的用量事件。
- **对应到标签页**：会话首行 `session_meta.cwd`（及逐回合 `turn_context.cwd`）与标签页当前目录一致即归属该标签；在 mooncli 里启动的会话优先按会话 ID 对应（notify 的 `thread-id`、文件名尾部 UUID、`session_meta.id` 三者一致）。
- **状态**：Codex 的配置文件（用户级 `~/.codex/config.toml` 顶层 `notify`，项目级配置里写会被忽略）支持在一轮回答结束时（`agent-turn-complete` 事件）调用指定程序，事件 JSON 作为最后一个命令行参数传入，不含用量数据，只作刷新触发。一键接入时把它设为 `mooncli-bridge codex-notify`（同样保留用户原有的 notify 命令并转调）。注意：该机制在 Codex 源码里已属 legacy（有移除 TODO），所以实现上以文件轮询为独立兜底，notify 只加速刷新。

## 5. 小窗设计

### 形态

| 形态 | 说明 |
| --- | --- |
| **浮动卡片**（默认） | 在窗口内浮动，可拖动，松手后吸附到最近的角落；不挡住光标所在行（光标靠近时自动挪开或变透明） |
| **嵌入侧栏** | 放进右侧面板的"Agent"页 |
| **独立小窗** | 拖出主窗口变成一个置顶的系统小窗，切到别的程序也能看到 |
| **状态栏模式** | 缩成窗口底部状态栏里的一小段：`◔ 5h 42% · 周 18% · ctx 63%` |

### 卡片内容

```
┌ Claude Code · Opus ─────────── 跟随当前标签 ▾ ┐
│ 上下文   ███████████░░░░░░  63%   126k/200k    │
│ 5 小时   ███████░░░░░░░░░░  42%   2h 18m 后重置 │
│ 本周     ███░░░░░░░░░░░░░░  18%   周四 09:00   │
│ ─────────────────────────────────────────────  │
│ ● 等待你确认      重构标签页布局       已运行 14m │
└───────────────────────────────────────────────┘
```

- **跟随当前标签**（默认）：显示当前标签页里的 Agent；当前标签没有 Agent 时显示最近活动的一个。
- **全部 Agent**：列出所有标签页里的 Agent，每个一行紧凑显示，点击跳转。
- 5 小时和周额度是**账号级**的，多个 Claude Code 标签页共用一组数据；上下文是**会话级**的，每个标签页各自不同。卡片上用分隔线区分两类。

### 颜色和提醒

| 用量 | 进度条颜色 |
| --- | --- |
| < 70% | 主题强调色 |
| 70–90% | 主题警告色 |
| > 90% | 主题危险色，数字加粗 |

- 上下文超过 85% 时，卡片上提示"快满了，可以考虑 /compact"。
- 额度用完时显示重置倒计时，到点后提示一次"额度已重置"。
- 阈值都可调；每种提醒都能单独关掉。

### 可定制项

```jsonc
"agentWidget": {
  "mode": "floating",              // floating | sidebar | window | statusbar
  "anchor": "bottom-right",
  "follow": "active-tab",          // active-tab | all
  "fields": ["context", "fiveHour", "weekly", "status"],  // 顺序即显示顺序
  "style": "bars",                 // bars（进度条）| rings（圆环）| compact（只有数字）
  "opacity": 0.92,
  "autoHide": "no-agent",          // never | no-agent（没有 Agent 时隐藏）| idle-30s
  "thresholds": { "warn": 0.7, "danger": 0.9 },
  "showEstimates": true            // 是否显示估算值
}
```

卡片背景用 `ui.surface1` 加 90% 不透明度和轻微背景模糊，浅色主题下像一张磨砂卡片，见 [08](08-art-direction.md)。

## 6. 隐私与开关

- 整个 Agent 功能默认**关闭**，第一次检测到 Claude Code 或 Codex 运行时，在标签页上提示一次"要开启用量小窗吗？"。
- mooncli 只在内存里保存最近的数据，不写入数据库；会话文件只读不写。
- 设置里可以分别关闭：读取会话文件、接收 statusLine 数据、接收 hooks。

## 7. 附录：实现状态与调研结论（2026-10-22，分支 feat/agent-monitor）

### 已实现

- `crates/mooncli-agent`：数据模型、Codex rollout 解析、Claude 转录解析（上下文 + 5 小时块估算）、statusLine/hooks spool 读取、接入合并逻辑（纯函数，带单元测试）。
- `mooncli-bridge`（同 crate 的 bin）：`statusline`（写 spool + 转调原命令 + 可选自带一行）、`hook`（写 spool）、`codex-notify`（写 spool + 转调原 notify），永远 0 退出、错误全吞。
- 主进程：`agent_usage` 采集命令（按设置的三个来源开关过滤）、`agent_bridge_path`、一键接入的预览/执行命令（写前备份 `*.mooncli-backup-<时间戳>`、精确撤销）；开启功能时给新标签页注入 `MOONCLI_AGENT_DIR`（`MOONCLI_SESSION_ID` 本来就有）。
- 界面：浮动小窗（跟随当前标签、四角停靠、不透明度、警告/危险阈值、全部会话列表、快照时间标注、已过重置点的窗口按「已重置」呈现）；设置页「Agent 用量」一节（总开关、三个来源开关、小窗外观、一键接入/断开含变更预览）。浮层打开时小窗让位，在本节里保留以便调外观时实时预览。
- 演示模式有对应的模拟数据，浏览器打开即可看效果。

### 未做（后续）

- 估算百分比的「校准」UI（第 2 节的设计）；小窗的侧栏 / 独立小窗 / 状态栏三种形态；浮动卡片的拖动吸附；额度重置提醒通知；多账号（多个 CODEX_HOME 已在数据层支持并合开展示，界面未做账号分组）。

### 调研结论（核实日期 2026-10-22，一手来源优先）

**Claude Code**
- statusLine 输入含 `rate_limits`：`five_hour` / `seven_day`，各含 `used_percentage`（0–100）与 `resets_at`（Unix 纪元秒）。自 **v2.1.80** 加入（官方 changelog 原文确认）；仅 claude.ai Pro/Max 订阅（及网关 spend limit）会话有，首次 API 响应后才出现，窗口过期后官方不再下发该窗口——小窗据此把已过重置点的窗口显示为「已重置」而不是陈旧百分比。
- statusLine 输入含 `context_window`：`total_input_tokens`（= input + cache_creation + cache_read，**不含 output**，与 `/context` 的口径差异官方有说明）、`context_window_size`（默认 200000，扩展上下文 1000000）、`used_percentage`、`current_usage`。上下文上限以它为准；只靠转录时 `[1m]` 后缀判 1M 没有一手依据，仅作回退启发。
- hooks 载荷里没有任何额度/上下文数字（逐项核对官方字段表）——状态可以、数字不行，数字只有 statusLine 和转录两条路。
- 5 小时窗口是「基于会话」的：由首条消息开启、持续 5 小时（帮助中心口径）；本地块估算按此建模（起点 = 首个事件时刻，不取整）。**额度百分比的本地估算不存在**：分母（套餐上限）从不下发到本地日志，这就是必须用 statusLine `rate_limits` 的原因。
- 第三方对照：ccusage 是纯本地路线（只算 token/成本，给不出百分比）；Claude-Code-Usage-Monitor 的官方百分数、CodexBar 一类工具靠的是读 OAuth 凭证调未公开接口（`/api/oauth/usage`，其 `resets_at` 还是 ISO 字符串，与 statusLine 的纪元秒不同）——都在第 1 节的红线之外，本项目不用。社区「文件桥」做法（statusLine 脚本把 JSON 落盘供其它程序读）与本实现的 spool 模式一致。

**Codex**
- rollout 文件（`~/.codex/sessions/年/月/日/rollout-*.jsonl`，归档在 `archived_sessions/`）的 `token_count` 事件（openai/codex 源码 `codex-rs/protocol` 的 `TokenCountEvent` 核实）：`info` 给 `total/last_token_usage` 与 `model_context_window`；`rate_limits`（`RateLimitSnapshot`）的 primary/secondary 各含 `used_percent`（f64）、`window_minutes`、`resets_at`（epoch 秒），另含 `plan_type`。这份快照是客户端从后端响应头解析后落盘的，与官方 `/status` 同源——**纯读文件就能拿到真实百分比**，现有纯本地工具（ccusage 等）没用这个字段所以给不出百分比。
- 窗口构成会被官方调整（2026 年内 5 小时窗口曾取消又对 Plus 恢复、Pro 至今只有周上限）：一律认 `window_minutes`，容忍任一窗口缺席；周上限的绝对数值官方未公布，不要用 token 数反推百分比。
- notify：用户级 `~/.codex/config.toml` 顶层 `notify` 数组；唯一真正分发的事件是 `agent-turn-complete`，JSON 以最后一个 argv 传入，含 `thread-id`/`cwd`，无用量；项目级配置里写 notify 会被忽略；源码已将其定性为 legacy（`legacy_notify.rs`，有移除 TODO），通知子进程不被等待——文件轮询必须留作兜底。
- 会话与标签页的对齐键：文件名尾部 UUID = `session_meta.id` = notify 的 `thread-id`。
- 第三方对照：CodexBar 的额度主路径是读 `~/.codex/auth.json` 调 `chatgpt.com/backend-api/wham/usage`（未公开接口，红线外）；「号池」类（CLIProxyAPI 等）的可借鉴点只有组织方式——一账号一目录、状态存 sidecar 不回写工具目录、被动观测优先——它们的凭证使用与轮换与展示型小窗是两回事，不引入。
