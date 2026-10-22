<p align="center"><img src="design/logo/mooncli.svg" width="96" height="96" alt="mooncli"></p>

<h1 align="center">mooncli</h1>

<p align="center"><b>为 AI Agent 时代做的 Windows 终端：PowerShell 7 内核，浅色优先，记得住命令，看得懂 Claude Code 和 Codex 在干什么。</b></p>

<p align="center">
<a href="https://github.com/moonmxi/mooncli/releases/latest"><img src="https://img.shields.io/github/v/release/moonmxi/mooncli" alt="Release"></a>
<a href="https://github.com/moonmxi/mooncli/actions/workflows/ci.yml"><img src="https://github.com/moonmxi/mooncli/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="LICENSE"><img src="https://img.shields.io/github/license/moonmxi/mooncli" alt="License"></a>
<img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-blue" alt="Platform">
</p>

<p align="center">
<a href="https://github.com/moonmxi/mooncli/releases/latest">下载最新版</a> ·
<a href="https://github.com/moonmxi/mooncli/releases/tag/nightly">预览版 nightly</a> ·
<a href="docs/README.md">文档</a> ·
<a href="README.en.md">English</a>
</p>

![月白主题：标签页采用了 Claude Code 会话的标题，Agent 正在等你确认](docs/images/hero.png)

## 为什么用 mooncli

**一眼看清 Agent 的额度和进度。** 卡片上固定列出 Claude Code 和 Codex 各自的 5 小时、7 天额度和重置倒计时；当前标签页在跑 Agent 时，再显示这个会话的上下文占用。一轮跑完或要你确认时，右下角弹系统通知，点一下就回到那个标签页。数据只走官方渠道，不碰登录凭证。

![Agent 额度卡片：Claude Code 与 Codex 的 5 小时、7 天额度，以及当前会话的上下文](docs/images/agent-widget.png)

**记得住你敲过的每条命令。** 历史库记下每条命令在哪个目录、哪台主机、成没成功、跑了多久。`Ctrl+Shift+R` 打开历史面板，按目录、主机、成败、收藏筛选；`Alt+/` 在提示符上弹出补全面板，历史、命令说明和 pwsh 自己的补全合在一起。

![历史面板：按目录、主机、成败筛选，还能看每条命令的统计](docs/images/history-panel.png)

**标签页看得懂里面在跑什么。** 自动显示目录、git 分支、正在跑的程序（看进程树，`npm run dev` 也认得出是 node）；沿用 Claude Code、Codex 自己设置的标题和图标。后台标签页跑完长命令、响铃、Agent 等你确认时，标签上亮点，窗口不在前台还会闪任务栏、发通知。

**浅色也好看。** 五套内置主题（月白、霜蓝、樱晨、青瓷，深色的夜航），颜色全部过了对比度检查；背景图、字体、三种缩放都能调，能导入 Windows Terminal 的配色。oh-my-posh、starship 的提示符原样显示。

![设置：五套主题都过了对比度检查，背景图、字体、缩放都能调](docs/images/settings.png)

## 功能一览

| | |
| --- | --- |
| Agent | Claude Code / Codex 的 5 小时与 7 天额度、当前会话上下文、完成与等你确认时的系统通知；一键接入（先预览、自动备份、可精确断开），也可以手动配置 |
| 命令 | 命令块（左侧圆点标成败、滚动条标记、`Ctrl+↑↓` 跳转、选中整条输出）、输出查找 `Ctrl+Shift+F`、历史面板、补全面板 |
| 标签页与分屏 | 左侧或顶部标签栏、拖动排序、置顶、改名、颜色；分屏 `Ctrl+Shift+E` / `Ctrl+Shift+O`；重开时恢复上次的标签页和目录 |
| 远程 | 识别 ssh、mosh、wsl、docker exec 等连接，连上就算完成，服务器上的每条命令单独计时、记进历史；按键原样透传，远程 TAB 补全不受影响 |
| 手感 | GPU 渲染、平滑滚动、智能复制粘贴、多行粘贴确认、`Ctrl+Shift+P` 命令面板、所有快捷键可改可录 |
| 数据 | 设置每天自动备份可恢复，整体导出导入；设置文件改了立即生效，带 JSON Schema 补全；日志只存本机，可一键打包 |
| 省心 | 没装 PowerShell 7 时打开就是安装引导；自带新版 ConPTY，老系统上 shell 集成也可靠 |

## 下载安装

到 [Releases](https://github.com/moonmxi/mooncli/releases/latest) 下载 `mooncli-setup-x64.exe` 直接运行。想尝鲜就用 [nightly](https://github.com/moonmxi/mooncli/releases/tag/nightly)，每次推送 main 自动更新。

需要 Windows 10 / 11 和 [PowerShell 7](https://aka.ms/PSWindows)（没装的话 mooncli 会引导你用 winget 一键安装）。

安装包目前**没有代码签名**，Windows 可能提示"未知发布者"：点 **更多信息 → 仍要运行** 即可（原因和计划见 [docs/10](docs/10-release.md)）。

### 开启 Agent 额度与完成提醒

1. 设置（`Ctrl+,`）→ Agent → 接入 Claude Code、接入 Codex。写入前会列出要改的内容，原文件自动备份。
2. 打开「显示 Agent 额度」。Codex 的额度不接入也能查（通过本机 Codex 的 app-server），Claude Code 的额度要接入后、下一次回复时出现（Pro / Max 订阅）。
3. 点「发送测试通知」确认右下角能弹出提醒。

细节见 [docs/07](docs/07-agent-monitor.md)。

## 文档

从 [docs/README.md](docs/README.md) 开始读。

| 文档 | 内容 |
| --- | --- |
| [00 愿景与范围](docs/00-vision.md) | 做什么、不做什么、设计原则 |
| [01 总体架构](docs/01-architecture.md) | 技术选型、进程模型、模块划分 |
| [02 终端内核](docs/02-terminal-core.md) | ConPTY、pwsh 启动、shell 集成协议、渲染 |
| [03 外观系统](docs/03-appearance.md) | 主题、字体、颜色、背景、配置格式 |
| [04 命令记忆与补全](docs/04-command-memory.md) | 历史库、历史面板、补全面板 |
| [05 使用体验](docs/05-interaction.md) | 滚动、选择、复制粘贴、快捷键、ssh |
| [06 智能标签页](docs/06-smart-tabs.md) | 标签布局、内容识别、标题与图标、提醒、分屏 |
| [07 Agent 额度与完成提醒](docs/07-agent-monitor.md) | 数据来源、合规边界、接入、卡片与通知 |
| [08 美术风格与版面](docs/08-art-direction.md) | 配色、字体、间距、各区域版面 |
| [09 实现计划](docs/09-roadmap.md) | 进度、剩下的工作、性能指标 |
| [10 发布与签名](docs/10-release.md) | 下载渠道、发版流程、代码签名 |

## 参与开发

需要：Rust（stable）、Node.js 22 + pnpm 10、PowerShell 7。

```powershell
cd app
pnpm install
pnpm tauri dev      # 启动桌面应用
pnpm dev            # 只在浏览器里调界面（模拟 shell），http://localhost:1420
pnpm test           # 界面逻辑测试
cd ..
cargo test          # 核心库测试；装了 pwsh 时会在真实 pwsh 里跑端到端测试
```

| 目录 | 内容 |
| --- | --- |
| `crates/mooncli-protocol` | 识别输出里的 shell 集成标记、会话状态机、远程连接识别、标题清理 |
| `crates/mooncli-history` | 命令历史库：过滤、导入、模糊搜索和排序 |
| `crates/mooncli-config` | 设置与主题：带注释的 JSON、主题继承、对比度检查 |
| `crates/mooncli-pty` | 伪终端会话（ConPTY）、查找和启动 pwsh、进程树 |
| `crates/mooncli-agent` | Agent 额度与上下文、完成提醒、一键接入，以及 `mooncli-bridge` |
| `app/src-tauri` | Tauri 主程序 |
| `app/src` | 界面（React + xterm.js） |
| `shell/pwsh/MoonCli.ps1` | pwsh 集成脚本 |

每次推送，GitHub Actions 会在 Windows 上编译、测试并打出安装包，发到 `nightly` 预发布版；推 `v*` 标签发正式版本。参与方式见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 隐私 / Privacy

除非你明确要求，mooncli 不会把任何信息发送到其它联网系统。命令历史、设置、背景图、Agent 数据都只保存在你自己的电脑上；「检查更新」只在你点按钮时访问 GitHub。

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

安装时如果电脑上没有 WebView2 运行库，安装程序会从微软下载它（受[微软隐私声明](https://privacy.microsoft.com/privacystatement)约束）。

## 许可证

[MIT](LICENSE)。附带的第三方组件：Symbols Nerd Font（MIT，`app/public/fonts/`）、微软 ConPTY（MIT，打包时下载）。
