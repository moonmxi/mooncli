<p align="center"><img src="design/logo/mooncli.svg" width="96" height="96" alt="mooncli"></p>

# mooncli

**一个为 Windows 做的第三方终端，底层跑 PowerShell 7（pwsh）。**

好看（尤其是浅色）、记得住命令、用着顺手、看得懂每个标签页里在干什么——尤其是当标签页里跑的是 Claude Code、Codex 这类 Agent 的时候。

*A Windows terminal for the AI-agent era: PowerShell 7 core, light-first themes, a memory for your commands, and tabs that understand what your coding agents are doing.* → [English README](README.en.md)

[![Release](https://img.shields.io/github/v/release/moonmxi/mooncli)](https://github.com/moonmxi/mooncli/releases)
[![CI](https://github.com/moonmxi/mooncli/actions/workflows/ci.yml/badge.svg)](https://github.com/moonmxi/mooncli/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/moonmxi/mooncli)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows-blue)

[下载正式版 v0.1.0](https://github.com/moonmxi/mooncli/releases/tag/v0.1.0) · [预览版 nightly](https://github.com/moonmxi/mooncli/releases/tag/nightly) · [文档](docs/README.md)

## 截图

![月白主题：标签页采用了 Claude Code 会话的标题，Agent 正在等你确认](docs/images/hero.png)

_标签页会自动采用 Agent 会话的标题和图标；Agent 等你确认时，标签上有提醒。_

![历史面板：按目录、主机、成败筛选，还能看每条命令的统计](docs/images/history-panel.png)

![设置：五套主题都过了对比度检查，背景图、字体、缩放都能调](docs/images/settings.png)

![夜航：唯一的深色主题](docs/images/theme-dark.png)

_截图出自浏览器演示模式（模拟 shell）；桌面版界面相同，跑的是真 pwsh。_

## 主要特点

### 已经能用的（v0.1.0）

- **浅色优先的外观系统**：内置五套主题——月白、霜蓝、樱晨、青瓷（浅色）和夜航（深色），颜色全部过了对比度检查；背景图（四张内置 + 自己导入）、字体、界面缩放都能改，也能跟随系统深浅色。
- **命令记忆**：自带历史库，记录命令在哪个目录、哪台主机、成没成功、跑了多久；`Ctrl+Shift+R` 打开历史面板，可按目录、标签页、收藏、成败、主机筛选，看详情和统计。
- **流畅的手感**：GPU 渲染、平滑滚动、智能复制粘贴、复制提醒、多行粘贴确认；ssh 到远程后按键原样透传，TAB 补全不会失灵；连上服务器就算这条命令完成，之后在服务器上执行的每条命令单独计时、记进历史。
- **会看内容的标签页**：标签栏可放左侧或顶部；自动显示目录、正在跑的程序；沿用 Claude Code / Codex 自己设置的标题和对应图标，Agent 等你确认时标签会提醒。
- **快捷键全可自定义**：每个操作都能改、能录，有冲突提示，带两套预设；`Ctrl+Shift+P` 打开命令面板。
- **数据在自己手里**：设置每天自动备份、可恢复，整体可导出/导入；没装 PowerShell 7 时打开就是安装引导，不会悄悄换成 5.1 将就。

### 还在做的

- 窗口材质（Mica / 亚克力）、主题编辑器、配色导入
- 补全面板、常驻候选面板、命令块装饰
- Agent 用量小窗：实时显示 Claude Code / Codex 的 5 小时额度、周额度和上下文占用
- 进程树识别

进度以 [docs/09](docs/09-roadmap.md#当前进度) 为准。

## 下载

- **正式版**：[Releases 页面](https://github.com/moonmxi/mooncli/releases)里的 v0.1.0，下载 `mooncli-setup-x64.exe` 直接运行安装。
- **预览版（nightly）**：[同页](https://github.com/moonmxi/mooncli/releases/tag/nightly)的 `mooncli-setup-x64.exe`，每次推送 main 自动更新，想尝鲜或帮忙测试用这个。

安装包目前**没有代码签名**，Windows 可能会提示"未知发布者"：在提示框点 **更多信息 → 仍要运行** 即可。

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
| [10 发布与签名](docs/10-release.md) | 去哪下载、为什么安装时会被拦、代码签名怎么配 |

## 开发

需要：Rust（stable）、Node.js 22 + pnpm 10、PowerShell 7。

```powershell
cd app
pnpm install
pnpm tauri dev      # 启动真正的应用
pnpm dev            # 只在浏览器里打开界面（用模拟 shell，方便调界面），http://localhost:1420
pnpm test           # 界面逻辑测试
cd ..\ncargo test          # 核心库测试；装了 pwsh 时会在真实 pwsh 里跑集成脚本的端到端测试
```

| 目录 | 内容 |
| --- | --- |
| `crates/mooncli-protocol` | 识别输出里的 shell 集成标记、会话状态机（按键归属）、标题清理 |
| `crates/mooncli-history` | 命令历史库：过滤、导入、模糊搜索和排序 |
| `crates/mooncli-config` | 设置与主题：带注释 JSON、主题继承、组件色、对比度检查 |
| `crates/mooncli-pty` | 伪终端会话（Windows 上是 ConPTY）、查找和启动 pwsh |
| `app/src-tauri` | Tauri 主程序：把上面几块接起来，提供给界面调用 |
| `app/src` | 界面：标签栏、终端、状态栏、布局规则 |
| `shell/pwsh/MoonCli.ps1` | pwsh 集成脚本 |

每次推送，GitHub Actions 会在 Windows 上编译、测试并打出安装包，直接发到 Release 页的 `nightly` 预发布版；推 `v*` 标签则发正式版本。参与开发见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 设计资源

- [design/themes/](design/themes/)：内置主题的配色文件
- [design/mockup/index.html](design/mockup/index.html)：版面原型，浏览器直接打开，可切换主题和标签栏位置
- [design/logo/](design/logo/)：图标源文件和各尺寸预览
- [tools/check_contrast.py](tools/check_contrast.py)：主题对比度检查，规则在 [resources/contrast-rules.json](resources/contrast-rules.json)
- [tools/tune_palette.py](tools/tune_palette.py)：按规则自动微调主题的 16 色
- [tools/fetch_conpty.py](tools/fetch_conpty.py)：下载安装包附带的新版 ConPTY

## 隐私 / Privacy

除非你明确要求，mooncli 不会把任何信息发送到其它联网系统。命令历史、设置、背景图都只保存在你自己的电脑上。

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

安装时如果电脑上没有 WebView2 运行库，安装程序会从微软下载它（受[微软隐私声明](https://privacy.microsoft.com/privacystatement)约束）。

## 许可证

[MIT](LICENSE)。附带的第三方组件：Symbols Nerd Font（MIT，`app/public/fonts/`）、微软 ConPTY（MIT，打包时下载）。
