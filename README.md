<p align="center"><img src="design/logo/mooncli.svg" width="96" height="96" alt="mooncli"></p>

# mooncli

一个为 Windows 做的第三方终端，底层跑 PowerShell 7（pwsh）。

它想解决的事情很简单：**好看（尤其是浅色）、记得住命令、用着顺手、看得懂每个标签页里在干什么**。

> 当前状态：早期开发中，日常能用，智能功能陆续加入。进度见 [docs/09](docs/09-roadmap.md#当前进度)。
>
> **下载**：[mooncli-setup-x64.exe](https://github.com/moonmxi/mooncli/releases/download/nightly/mooncli-setup-x64.exe)（每次推送自动更新的预览版）。正式版本在 [Releases](https://github.com/moonmxi/mooncli/releases) 页面，正在申请 SignPath 的开源免费签名，签好之前 Windows 可能会提示"未知发布者"，见 [docs/10](docs/10-release.md) 和下面的[代码签名政策](#代码签名政策--code-signing-policy)。

## 主要特点（计划中）

- **浅色优先的外观系统**：内置 4 套浅色主题 + 1 套深色主题，颜色全部过了对比度检查；字体、配色、背景图、窗口材质（Mica / 亚克力）都能改。
- **命令记忆**：自带历史库，记录命令在哪个目录、哪台主机、成没成功、跑了多久；`Ctrl+Shift+R` 打开历史面板，可按目录、标签页、收藏、成败、主机筛选，可去重，也能唤出"当前输了一半的命令"的所有候选，并可把候选面板常驻在侧边。
- **流畅的手感**：GPU 渲染、平滑滚动、智能复制粘贴；ssh 到远程后按键原样透传，TAB 补全不会失灵；连上服务器就算这条命令完成，之后在服务器上执行的每条命令也会单独计时、记进历史。
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
| [10 发布与签名](docs/10-release.md) | 去哪下载、为什么安装时会被拦、代码签名怎么配 |

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

每次推送，GitHub Actions 会在 Windows 上编译、测试并打出安装包，直接发到 Release 页的 `nightly` 预发布版。

## 设计资源

- [design/themes/](design/themes/)：内置主题的配色文件
- [design/mockup/index.html](design/mockup/index.html)：版面原型，浏览器直接打开，可切换主题和标签栏位置
- [design/logo/](design/logo/)：图标源文件和各尺寸预览
- [tools/check_contrast.py](tools/check_contrast.py)：主题对比度检查，规则在 [resources/contrast-rules.json](resources/contrast-rules.json)
- [tools/tune_palette.py](tools/tune_palette.py)：按规则自动微调主题的 16 色
- [tools/fetch_conpty.py](tools/fetch_conpty.py)：下载安装包附带的新版 ConPTY

## 代码签名政策 / Code signing policy

正式版本的安装包计划由 SignPath 免费签名（开源项目计划，申请中）：

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- 只签由本仓库源码、在 GitHub Actions 上构建出来的安装包；每次签名都要由下面的"批准人"在 SignPath 上手动批准。
- 团队角色：
  - 提交者与审阅者（Committers and reviewers）：[moonmxi](https://github.com/moonmxi)
  - 批准人（Approvers）：[moonmxi](https://github.com/moonmxi)
- 每日构建（`nightly`）不签名。

## 隐私 / Privacy

除非你明确要求，mooncli 不会把任何信息发送到其它联网系统。命令历史、设置、背景图都只保存在你自己的电脑上。

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

安装时如果电脑上没有 WebView2 运行库，安装程序会从微软下载它（受[微软隐私声明](https://privacy.microsoft.com/privacystatement)约束）。

## 许可证

[MIT](LICENSE)。附带的第三方组件：Symbols Nerd Font（MIT，`app/public/fonts/`）、微软 ConPTY（MIT，打包时下载）。
