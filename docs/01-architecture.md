# 01 总体架构

## 1. 技术选型

### 结论

| 层 | 选择 | 理由 |
| --- | --- | --- |
| 应用外壳 | **Tauri 2**（Rust + WebView2） | 比 Electron 省内存、启动快；Rust 端直接调 Win32 / ConPTY；Windows 11 自带 WebView2 |
| 界面 | **TypeScript + React 18 + Vite** | 面板、设置、命令面板这些复杂界面写起来快；生态成熟 |
| 终端渲染 | **xterm.js 5 + WebGL 渲染器** | VS Code、Tabby、Hyper 都在用，最成熟的网页终端；GPU 渲染够快 |
| 伪终端 | **ConPTY**，通过 `portable-pty` crate 调用，并**自带新版 conpty.dll + OpenConsole.exe** | 和 Windows Terminal、WezTerm 一致；自带新版能避开旧系统 ConPTY 的 bug |
| 数据存储 | **SQLite**（rusqlite） | 历史库要按目录、时间、退出码查，SQLite 最合适 |
| 模糊匹配 | **nucleo** crate | Helix 编辑器用的匹配器，快，十万条历史毫秒级 |
| 配置 | **JSON + JSON Schema** | 用 VS Code 改配置能自动补全和校验；设置界面也读写同一个文件 |

### 为什么不选别的

| 方案 | 没选的原因 |
| --- | --- |
| 原生 GPU 渲染（像 WezTerm / Alacritty 用 wgpu 自己画） | 渲染最快，但面板、设置、命令面板全得自己画，工作量是现在的几倍。我们的重点是外观定制和面板交互，网页技术更划算。 |
| Electron（像 Tabby / Hyper） | 能做，但内存和安装包都大不少。Tauri 的 Rust 端也更适合做 ConPTY、进程树查询这些系统活。 |
| WinUI 3 / WPF（像 Windows Terminal） | 只能用 C#/C++，界面定制不如网页灵活，渲染还得自己接 DirectX。 |

### 已知代价

- WebView2 和 Rust 之间传终端输出有开销。对策：用 Tauri 2 的 `Channel` 流式传输，输出按 ~4ms 攒一批再发（见 [02](02-terminal-core.md)）。
- xterm.js 的连字（ligature）支持依赖插件，和 WebGL 渲染器的配合 **【待核实】**，M3 做原型验证。

## 2. 进程模型

```
┌──────────────────────────── mooncli.exe（Tauri 主进程，Rust）────────────────────────────┐
│                                                                                          │
│  会话管理  ── 每个标签页一个 Session ──►  ConPTY ──► pwsh.exe ──► 子进程（ssh、claude…）   │
│     │                                     ▲                                              │
│     │ 输出流 / 事件                        │ 输入                                         │
│     ▼                                     │                                              │
│  IPC 层（Tauri commands + Channels）◄──────┘                                              │
│     │                                                                                    │
│  历史库（SQLite） 进程探测  配置/主题监听  Agent 数据采集  本地命名管道服务                  │
│                                                                    ▲                     │
└────────────────────────────────────────────────────────────────────┼─────────────────────┘
          │ WebView2                                                 │
          ▼                                                          │
   界面（React + xterm.js）                                 mooncli-bridge.exe（小命令行工具）
   标签栏 / 终端 / 面板 / 小窗 / 设置                         被 Claude Code 的 statusLine、hooks 调用
```

- **一个主进程**，所有标签页共用。每个标签页（或分屏里的每个窗格）对应一个 `Session`，各自有一个 ConPTY 和一个 pwsh。
- **mooncli-bridge.exe**：几百 KB 的小程序，给 Claude Code 的 statusLine 和 hooks 调用，把数据通过命名管道 `\\.\pipe\mooncli-<用户SID>` 送回主进程。单独做成程序是因为 Agent 那边只能调用命令。
- 每个 pwsh 启动时带上环境变量 `MOONCLI_SESSION_ID`，子进程（包括 claude、codex、bridge）会继承它，所以 bridge 送回的数据能对上是哪个标签页。

## 3. 模块划分

### Rust 端（`crates/`）

| crate | 职责 |
| --- | --- |
| `mooncli-pty` | 封装 ConPTY：创建、读写、改尺寸、关闭；加载自带的 conpty.dll |
| `mooncli-session` | 会话生命周期；输出批处理；解析 mooncli 自定义 OSC 序列；进程树探测 |
| `mooncli-history` | 历史库读写、导入 PSReadLine 历史、排序和模糊搜索 |
| `mooncli-config` | 读配置和主题、校验、监听文件变化、热更新 |
| `mooncli-agent` | Agent 数据采集：bridge 管道服务、Codex 会话文件监听、用量汇总 |
| `mooncli-bridge` | 上面说的小命令行工具（独立二进制） |
| `app/src-tauri` | Tauri 主程序，把上面拼起来，定义 IPC 接口 |

### 界面端（`app/src/`）

| 目录 | 职责 |
| --- | --- |
| `terminal/` | xterm.js 封装、插件加载、键盘处理、OSC 处理、命令块装饰 |
| `tabs/` | 标签栏（左侧 / 顶部两种布局）、分屏、拖拽 |
| `panels/` | 历史面板、补全面板、常驻侧栏 |
| `agent/` | Agent 额度卡片 |
| `theme/` | 主题加载、CSS 变量注入、背景图层、配色导入 |
| `settings/` | 设置界面（读写同一个 settings.json） |
| `palette/` | 命令面板（Ctrl+Shift+P） |

### pwsh 端（`shell/pwsh/`）

- `MoonCli.psm1`：shell 集成模块。包住用户原来的 prompt，发出提示符/命令/目录标记；注册补全请求的按键处理。详见 [02](02-terminal-core.md)。

## 4. 主要数据流

**敲一条命令：**

1. 键盘输入 → xterm.js → IPC → ConPTY → pwsh（PSReadLine 处理编辑）。
2. 回车后 shell 集成发出"命令开始 + 命令文本"标记 → mooncli 记下开始时间、目录。
3. 命令结束，发出"命令结束 + 退出码"标记 → 写入历史库，标签页刷新状态。

**唤出补全面板（Ctrl+Space）：**

1. 界面确认当前在本机 pwsh 提示符上（见 [05](05-interaction.md) 的"按键归属"）。
2. 同时向多个来源要候选：历史库（Rust 端）、路径（Rust 端读本地文件）、命令说明书（界面端）、pwsh 自己的补全（发请求给 pwsh）。
3. 结果合并排序后显示；选中后把文字送进 pwsh 的输入行。

**Agent 额度与提醒（详见 [07](07-agent-monitor.md)）：**

1. Claude Code 刷新状态栏、触发 hooks，Codex 一轮结束调 notify，都会运行 `mooncli-bridge`；它把数据写进 mooncli 数据目录下的小文件（带上 `MOONCLI_SESSION_ID`，知道是哪个标签页）。
2. 界面每 5 秒问一次主进程；主进程读这些文件、通过 `codex app-server` 查 Codex 额度，只为当前标签页正在跑的 Agent 找上下文。
3. 一轮结束时 bridge 交来提醒，主进程转给界面，界面按你在不在看那个标签页决定弹不弹系统通知。

## 5. 仓库目录结构

```
mooncli/
├─ app/
│  ├─ src/                 界面（React + TS）
│  └─ src-tauri/           Tauri 主程序（Rust）
├─ crates/                 Rust 库（见上表）
├─ shell/pwsh/             pwsh 集成模块
├─ resources/
│  ├─ conpty/              自带的 conpty.dll、OpenConsole.exe（构建时从 NuGet 下载，不进 git）
│  ├─ themes/              内置主题（来自 design/themes）
│  ├─ specs/               命令说明书（补全用）
│  └─ backgrounds/         内置背景图
├─ schema/                 settings 和 theme 的 JSON Schema
├─ design/                 设计稿、主题源文件、原型
├─ docs/                   设计文档
└─ tools/                  开发脚本（对比度检查等）
```

## 6. 用户数据放哪

| 内容 | 位置 |
| --- | --- |
| 配置 | `%APPDATA%\mooncli\settings.json` |
| 自定义主题 | `%APPDATA%\mooncli\themes\*.json` |
| 背景图 | `%APPDATA%\mooncli\backgrounds\` |
| 历史库 | `%LOCALAPPDATA%\mooncli\history.db` |
| 日志 | `%LOCALAPPDATA%\mooncli\logs\`（滚动保留 7 天） |

配置放 Roaming 方便同步，历史库放 Local（体积大、不该漫游）。

## 7. 安全边界

- 命名管道只允许当前用户访问（设置 ACL）。
- 界面端不直接执行系统命令，所有系统能力走白名单 IPC 接口。
- 关掉 WebView2 的远程调试、禁止导航到外部网址；链接一律用系统浏览器打开。
- 历史库可能含敏感命令：提供"不记录"规则（见 [04](04-command-memory.md)）和一键清空。
