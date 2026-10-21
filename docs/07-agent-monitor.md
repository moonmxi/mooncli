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
| 模型 | statusLine 输入的 `model.display_name` | 会话文件里的配置记录 |
| 上下文占用 | ① statusLine 输入里有上下文字段就直接用 **【待核实】**；② 否则读 `transcript_path` 指向的会话记录，取最后一条回复的 token 用量（输入 + 缓存读取 + 缓存写入）÷ 模型上下文上限 | 会话文件里 `token_count` 事件的用量和 `model_context_window` **【待核实】** |
| 5 小时额度 | ① statusLine 输入里有额度字段就用 **【待核实】**；② 否则为"估算"：汇总本机所有会话记录在当前 5 小时窗口内的用量（社区工具 ccusage 用的就是这个方法），显示用量和窗口重置时间，百分比需要用户校准（见下） | 会话文件里 `token_count` 事件带的 `rate_limits.primary`（已用百分比、窗口长度、重置时间）**【待核实】** |
| 周额度 | 同上，① 官方字段；② 估算 + 校准 | `rate_limits.secondary` **【待核实】** |
| 本次花费 | statusLine 输入的 `cost.total_cost_usd`（API 计费用户才有意义） | 不显示 |
| 状态（运行/等待/完成） | hooks：`Notification`、`Stop`、`SessionStart`、`SessionEnd` 等 | `notify` 配置（一轮结束时调用指定程序） |

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

1. 读标准输入的 JSON，连同环境变量 `MOONCLI_SESSION_ID` 一起发给 mooncli 主进程（命名管道，100ms 超时，发不出去就算了，不阻塞）。
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

- **用量**：`mooncli-agent` 监听 `~/.codex/sessions/` 目录（**【待核实】**路径和文件格式），有新内容追加时只读新增部分，解析其中的用量事件。
- **对应到标签页**：在某个标签页的进程树里发现 codex 进程后，找"该进程启动后新建的、记录的工作目录和标签页当前目录一致"的会话文件。
- **状态**：Codex 的配置文件支持 `notify`，在一轮回答结束时调用指定程序并传入 JSON。一键接入时把它设为 `mooncli-bridge codex-notify`（同样保留用户原有的 notify 命令并转调）。**【待核实】**

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
