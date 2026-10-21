# mooncli

一个为 Windows 做的第三方终端，底层跑 PowerShell 7（pwsh）。

它想解决的事情很简单：**好看（尤其是浅色）、记得住命令、用着顺手、看得懂每个标签页里在干什么**。

> 当前状态：M0 骨架完成，M1（能用的终端）和 M2（shell 集成）大部分完成。进度见 [docs/09](docs/09-roadmap.md#当前进度)。还没有在 Windows 真机上验证过，CI 会打出安装包供试用。

## 主要特点（计划中）

- **浅色优先的外观系统**：内置 4 套浅色主题 + 1 套深色主题，颜色全部过了对比度检查；字体、配色、背景图、窗口材质（Mica / 亚克力）都能改。
- **命令记忆**：自带历史库，记录命令在哪个目录、成没成功、跑了多久；快捷键唤出全部历史（可去重），也能唤出"当前输了一半的命令"的所有候选，并可把候选面板常驻在侧边。
- **流畅的手感**：GPU 渲染、平滑滚动、智能复制粘贴；ssh 到远程后按键原样透传，TAB 补全不会失灵。
- **会看内容的标签页**：标签栏可放左侧或顶部；自动显示目录、正在跑的程序；沿用 Claude Code / Codex 自己设置的标题和对应图标；Agent 等你确认时标签会提醒。
- **Agent 用量小窗**：用 Claude Code 和 Codex 官方提供的扩展点和它们自己写在本机的数据，实时显示 5 小时额度、周额度和上下文占用。

## 文档

从 [docs/README.md](docs/README.md) 开始读，那里有阅读顺序。

| 文档 | 内容 |
| --- | --- |
| [00 愿景与范围](docs/00-vision.md) | 做什么、不做什么、设计原则 |
| [01 总体架构](docs/01-architecture.md) | 技术选型、进程模型、模块划分、目录结构 |
| [02 终端内核](docs/02-terminal-core.md) | ConPTY、pwsh 启动、shell 集成协议、渲染 |
| [03 外观系统](docs/03-appearance.md) | 主题、字体、颜色、背景、窗口材质、配置格式 |
| [04 命令记忆与补全](docs/04-command-memory.md) | 历史库、历史面板、补全面板、常驻面板 |
| [05 使用体验](docs/05-interaction.md) | 滚动、选择、复制粘贴、快捷键、ssh |
| [06 智能标签页](docs/06-smart-tabs.md) | 标签布局、内容识别、标题与图标、提醒 |
| [07 Agent 用量小窗](docs/07-agent-monitor.md) | 数据来源、合规边界、小窗设计 |
| [08 美术风格与版面](docs/08-art-direction.md) | 配色、字体、间距、各区域版面规格 |
| [09 实现计划](docs/09-roadmap.md) | 里程碑、任务拆分、验收标准、风险 |

## 开发

需要：Rust（stable）、Node.js 22 + pnpm 10、PowerShell 7。

```powershell
cd app
pnpm install
pnpm tauri dev      # 启动真正的应用
pnpm dev            # 只在浏览器里打开界面（用模拟 shell，方便调界面），http://localhost:1420
pnpm test           # 界面逻辑测试
cd ..
cargo test          # 核心库测试；装了 pwsh 时会在真实 pwsh 里跑集成脚本的端到端测试
```

| 目录 | 内容 |
| --- | --- |
| `crates/mooncli-protocol` | 识别输出里的 shell 集成标记、会话状态机（按键归属）、标题清理 |
| `crates/mooncli-history` | 命令历史库：过滤、导入、模糊搜索和排序 |
| `crates/mooncli-config` | 设置和主题：带注释 JSON、主题继承、组件色、对比度检查 |
| `crates/mooncli-pty` | 伪终端会话（Windows 上是 ConPTY）、查找和启动 pwsh |
| `app/src-tauri` | Tauri 主程序：把上面几块接起来，提供给界面调用 |
| `app/src` | 界面：标签栏、终端、状态栏、布局规则 |
| `shell/pwsh/MoonCli.ps1` | pwsh 集成脚本 |

每次推送，GitHub Actions 会在 Windows 上编译并打出安装包，在 Actions 页面的 `mooncli-windows-installer` 里下载。

## 设计资源

- [design/themes/](design/themes/)：内置主题的配色文件
- [design/mockup/index.html](design/mockup/index.html)：版面原型，浏览器直接打开，可切换主题和标签栏位置
- [tools/check_contrast.py](tools/check_contrast.py)：主题对比度检查脚本，`python3 tools/check_contrast.py`
