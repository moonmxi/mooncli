# 04 命令记忆与补全

## 1. 先看清楚已经有什么

pwsh 自带的 **PSReadLine** 已经做得不错，mooncli 不重复造轮子，而是**补它没有的部分**：

| 能力 | PSReadLine 自带 | mooncli 补充 |
| --- | --- | --- |
| 历史记录 | 一个纯文本文件，只有命令本身 | 历史库：记录目录、退出码、耗时、时间、会话、主机 |
| 输入时的灰色联想 | 有（InlineView），按 → 接受 | 保留不动；可选把 mooncli 的历史库接进去（见第 5 节） |
| 联想列表 | 有（ListView，F2 切换） | 保留不动 |
| 搜索历史 | `Ctrl+R` 逐条反向搜索 | 图形化历史面板：模糊搜索、去重、按目录筛选 |
| TAB 补全 | 有，调用 pwsh 自己的补全 | 保留不动；另外提供图形化补全面板，合并多个来源 |

**原则：TAB、→、F2 这些 PSReadLine 的按键不改**，用户原来的习惯全部保留。mooncli 只占用额外的快捷键。

## 2. 历史库

### 记什么

```sql
CREATE TABLE commands (
  id          INTEGER PRIMARY KEY,
  text        TEXT NOT NULL,         -- 命令原文，多行命令完整保存
  cwd         TEXT,                  -- 执行时的目录
  exit_code   INTEGER,               -- NULL 表示没拿到（比如没有 shell 集成）
  started_at  INTEGER NOT NULL,      -- Unix 毫秒
  duration_ms INTEGER,
  session_id  TEXT,                  -- 哪个标签页
  host        TEXT,                  -- 本机名，或 ssh 远端主机名
  shell       TEXT,                  -- pwsh / bash / zsh …
  source      TEXT NOT NULL          -- live（实时记录）| import（从 PSReadLine 导入）
);
CREATE INDEX idx_cmd_text ON commands(text);
CREATE INDEX idx_cmd_cwd  ON commands(cwd, started_at);
```

- 数据来源：shell 集成发出的 `cmd`（命令文本）+ `133;C`（开始）+ `133;D`（结束和退出码）+ `OSC 7`（目录）。
- **首次启动导入** PSReadLine 的历史文件（`(Get-PSReadLineOption).HistorySavePath`），标记为 `import`，这些记录没有目录和退出码。
- ssh 到远程后，如果远程 shell 也装了集成（见 [05](05-interaction.md)），远程命令同样入库，`host` 记远端主机名。

### 不记什么

- **以空格开头的命令**不记（和 bash 的 `ignorespace` 习惯一致）。
- 匹配敏感规则的不记，默认规则：
  - 含 `password=`、`--password`、`token=`、`secret=`、`api[_-]?key` 的
  - 含常见密钥格式的（`sk-…`、`ghp_…`、`AKIA…` 等）
  - `ConvertTo-SecureString -AsPlainText` 这类
- 用户可以在设置里加自己的规则（正则），也可以按目录关闭记录。
- 面板里任意一条都能右键"删除"，也能"清空某个目录下的全部历史"。

### 搜索速度

每次按键都要重新搜一遍，所以速度是硬指标（< 16ms）。实测（`crates/mooncli-history` 的性能测试，10 万条记录、5000 条不同命令、发布构建）：

| 做法 | 逐字输入时最慢的一次 |
| --- | --- |
| 每次在 SQLite 里 `GROUP BY` 汇总再打分 | 约 150ms |
| 内存索引：按命令文本汇总，写入时增量更新 | 约 32ms |
| 内存索引 + 先只算分数、选出前 200 名再算高亮位置 | **约 7ms** |

所以默认范围（全部历史、去重）走内存索引；带筛选条件（当前目录、当前会话、只看成功、指定主机）的查询结果集小，直接走 SQL。建索引约 120ms，在启动时后台完成。

### 排序

候选的分数由几部分加起来，权重可调：

| 因素 | 说明 |
| --- | --- |
| 匹配度 | 模糊匹配得分（nucleo），前缀匹配额外加分 |
| 常用度 | 用得越多越靠前，但随时间衰减（最近一周的次数权重最高） |
| 同目录 | 在当前目录执行过的命令加分；在当前 git 仓库里执行过的也加分 |
| 成功过 | 退出码为 0 的加分；只失败过的降权 |
| 刚用过 | 本会话最近 10 条加分 |

## 3. 历史面板（全部记忆命令）

**快捷键：`Ctrl+Shift+R`**（不占用 PSReadLine 的 `Ctrl+R`；用户可以在设置里换成 `Ctrl+R`）。

```
┌ 历史 ─────────────────────────────────────────── [去重 ✓] [Alt+D] ┐
│ 🔍 git pu                                                          │
│ 范围： 全部 · 当前目录 · 当前会话 · 只看成功    主机：全部 ▾        │
├────────────────────────────────────────────────────────────────────┤
│ ▸ git push origin main                    ~/mooncli   12 次  2 分钟前│
│   git pull --rebase                       ~/mooncli    8 次  1 小时前│
│   git push -u origin feat/tabs            ~/mooncli    1 次  昨天    │
│   git push --force-with-lease  ✗          ~/blog       1 次  3 天前  │
├────────────────────────────────────────────────────────────────────┤
│ Enter 填入   Shift+Enter 直接执行   Ctrl+C 复制   Del 删除   Tab 预览 │
└────────────────────────────────────────────────────────────────────┘
```

- **去重开关**（`Alt+D`）：开着时相同命令只显示一条，附带次数和最近时间；关掉时按时间顺序显示每一次执行，可以看到每次的目录和结果。默认开，记住上次的选择。
- **范围**：`Alt+1~4` 快速切换。
- **预览**（`Tab`）：右侧展开详情——完整多行命令、每次执行的时间/目录/耗时/退出码。
- **填入 vs 执行**：`Enter` 只把命令放进输入行（可以再改），`Shift+Enter` 直接执行。
- 位置：默认在终端上方浮出（类似 VS Code 的命令面板），可设置为从侧边滑出。
- 面板打开时，终端区域不变暗，只在面板外加一层很淡的蒙版，保持"还在终端里"的感觉。

## 4. 补全面板（输到一半的命令）

### 唤出

**快捷键：`Ctrl+Space`**。显示"当前输入行在光标处能接的所有选项"。

> 注意：PSReadLine 在 Windows 模式下默认把 `Ctrl+Space` 绑定为 `MenuComplete`（在终端里画一个文字版候选菜单）。mooncli 的补全面板就是它的图形化加强版，所以默认接管这个键；设置里有"`Ctrl+Space` 交给 PSReadLine"的开关，打开后面板改用 `Ctrl+Shift+Space`。

```
 PS ~/mooncli> git ch█
               ┌──────────────────────────────────────────────┐
               │ ⎇ checkout       切换分支或恢复文件    命令说明 │
               │ ⎇ cherry-pick    挑选提交应用到当前分支 命令说明 │
               │ ⎇ cherry         找出未合并的提交       命令说明 │
               │ ↺ checkout -b feat/agent-widget   历史 · 3 次  │
               │ ↺ checkout main                   历史 · 20 次 │
               └──────────────────────────────────────────────┘
               ↑↓ 选择  Enter 填入  → 接受并继续  Esc 关闭  Ctrl+P 钉住
```

### 候选从哪来

四个来源并行请求，先到先显示，后到的插入并重新排序（不跳动：已经显示的项只在用户没在操作时才重排）：

| 来源 | 内容 | 在哪算 | 速度 |
| --- | --- | --- | --- |
| **pwsh 补全** | pwsh 自己的补全结果：cmdlet、参数、变量、路径、模块里的自定义补全 | 在 pwsh 里算 | 50–300ms |
| **历史** | 以当前输入为前缀或模糊匹配的历史命令 | Rust 端查历史库 | < 10ms |
| **命令说明书** | `git`、`npm`、`docker`、`cargo`、`kubectl` 等外部程序的子命令和参数，带中文说明 | 界面端 | < 5ms |
| **路径** | 当前目录下的文件和文件夹 | Rust 端读文件系统 | < 10ms |

**pwsh 补全怎么拿**（参考 VS Code 的终端补全）：

1. 集成脚本用 `Set-PSReadLineKeyHandler` 注册一个用户基本不会按到的组合键（比如 `F12,F11` 连按）。
2. 用户按 `Ctrl+Space` 时，mooncli 往 pwsh 里**悄悄发送这个组合键**。
3. pwsh 里的处理函数读出当前输入行和光标位置，调用 `TabExpansion2`，把结果编码成私有序列 `completions` 输出。
4. mooncli 拦下这段序列，不显示在屏幕上，解析后放进面板。

```powershell
Set-PSReadLineKeyHandler -Chord 'F12,F11' -ScriptBlock {
    $line = $null; $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
    $r = TabExpansion2 -inputScript $line -cursorColumn $cursor
    $items = $r.CompletionMatches | Select-Object -First 200 | ForEach-Object {
        @{ t = $_.CompletionText; l = $_.ListItemText; k = "$($_.ResultType)"; d = $_.ToolTip }
    }
    [Console]::Write((Priv 'completions' @{
        start = $r.ReplacementIndex; len = $r.ReplacementLength; items = @($items) }))
}
```

**命令说明书**：用 Fig 开源的 autocomplete specs（MIT 协议，inshellisense 也在用）转换成 JSON 随程序附带，挑最常用的 100 个左右；中文说明我们自己补，社区可以贡献。

### 选中之后

- `Enter`：用候选替换光标处的那个词（替换范围来自 pwsh 返回的 `ReplacementIndex/Length`；其他来源按"光标前的当前词"计算）。
- `→`：接受并保持面板打开，继续补下一段（比如 `git` → `checkout` → 分支名）。
- 往 pwsh 写入替换文本的方式：先发退格删掉要替换的字符，再发新文字。比直接改 PSReadLine 缓冲区简单可靠，也能撤销（`Ctrl+Z`）。

### 常驻面板（钉住）

在补全面板里按 `Ctrl+P`，或在侧栏打开"补全"页，面板就**常驻在右侧**，随输入实时刷新。

```
┌ 终端 ──────────────────────────────────┬ 补全 📌 ─────────────────┐
│ PS ~/mooncli> docker run -█            │ 参数                      │
│                                        │  -d  --detach  后台运行    │
│                                        │  -e  --env     设置环境变量│
│                                        │  -p  --publish 端口映射    │
│                                        │  -v  --volume  挂载目录    │
│                                        │ 历史                      │
│                                        │  docker run -d -p 80:80 … │
└────────────────────────────────────────┴───────────────────────────┘
```

- **怎么知道用户输了什么**：mooncli 知道提示符结束的位置（`133;B`）和光标位置，直接从屏幕缓冲区读出这段文字，不用麻烦 pwsh。读不准的情况（多行输入、PSReadLine 在显示联想列表时）用 `buffer` 私有序列校正。
- **刷新节奏**：历史、说明书、路径三个来源每次按键后 80ms 防抖刷新；pwsh 补全只在停顿 400ms 后请求一次，并且默认关闭（"常驻面板也请求 pwsh 补全"开关），因为悄悄发按键在用户快速打字时有和输入抢顺序的风险，需要实测确认安全。**【待核实】**
- 常驻时用鼠标点击候选也能填入；`Alt+↑↓` 在不离开输入行的情况下选择候选，`Alt+Enter` 填入。
- 离开提示符（命令开始执行、进入 ssh、打开 vim）时，常驻面板自动变灰，显示"当前程序不在 pwsh 提示符上"，不再刷新。

## 5. 把历史库接进 PSReadLine（进阶，M4 之后）

PSReadLine 2.2+ 支持"预测插件"（实现 `ICommandPredictor` 接口的 C# 模块）。我们可以写一个 `MoonCli.Predictor` 模块，让 PSReadLine 原生的灰色联想和 F2 列表也用上 mooncli 历史库的排序（同目录优先、成功优先）。

- 好处：完全用 PSReadLine 原生的显示和按键，零冲突，ssh 场景天然不受影响。
- 通信：模块通过命名管道问 mooncli 主进程要预测结果，要求 20ms 内返回（PSReadLine 对预测插件有超时限制）。
- 用户可选：`PredictionSource` 设为 `HistoryAndPlugin` 时生效，不想用就不开。

## 6. 快捷键汇总

| 快捷键 | 作用 | 什么时候生效 |
| --- | --- | --- |
| `Ctrl+Shift+R` | 历史面板 | 任何时候（在 ssh 里打开时，默认范围切到"当前主机"） |
| `Ctrl+Space` | 补全面板（接管 PSReadLine 的 MenuComplete，可还原） | 只在本机 pwsh 提示符上；其它时候原样发给程序 |
| `Ctrl+P`（面板内） | 钉住 / 取消钉住 | 面板打开时 |
| `Alt+D`（面板内） | 切换去重 | 历史面板打开时 |
| `Alt+↑↓` / `Alt+Enter` | 选择 / 填入常驻面板的候选 | 常驻面板打开且在提示符上 |
| `Tab`、`→`、`F2`、`Ctrl+R` | 保持 PSReadLine 原样 | — |

所有快捷键都能在设置里改。改动时检查冲突并提示。
