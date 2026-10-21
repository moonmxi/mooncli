# 02 终端内核

这一篇讲"终端本身"怎么工作：怎么启动 pwsh、输入输出怎么走、mooncli 怎么知道 pwsh 里发生了什么。

## 1. ConPTY

ConPTY 是 Windows 提供的"伪终端"：我们创建一个，它会给 pwsh 一个假的控制台，然后把 pwsh 的输出翻译成标准的终端字符序列（VT 序列）给我们，我们再交给 xterm.js 显示。

### 用自带的新版 ConPTY

系统自带的 ConPTY 跟着 Windows 版本走，老系统上有不少已修复的 bug（重绘错乱、改尺寸丢内容等）。Windows Terminal 和 WezTerm 的做法是**随程序附带新版 `conpty.dll` 和 `OpenConsole.exe`**，我们照做：

- 构建时从微软官方的 `Microsoft.Windows.Console.ConPTY` NuGet 包取这两个文件，放在 `mooncli.exe` 旁边。
- 启动时优先加载自带的 `conpty.dll`，失败就退回系统 `kernel32` 里的 `CreatePseudoConsole`。
- 设置里留一个开关"使用系统 ConPTY"，方便排查问题。

### 创建参数

| 项 | 做法 |
| --- | --- |
| 尺寸 | 用 xterm.js 算出的行列数创建；窗口改尺寸时防抖 50ms 再调 `ResizePseudoConsole` |
| 标志位 | 先只开 `PSEUDOCONSOLE_RESIZE_QUIRK`（避免改尺寸时重复绘制）。`WIN32_INPUT_MODE` 见下文 |
| 环境变量 | 继承用户环境，另加 `TERM_PROGRAM=mooncli`、`TERM_PROGRAM_VERSION`、`MOONCLI_SESSION_ID`、`COLORTERM=truecolor` |
| 工作目录 | 新标签页默认用"当前标签页所在目录"，可在设置里改成固定目录或用户主目录 |

### 键盘编码（WIN32_INPUT_MODE）

VT 序列表达不了所有按键组合，比如 `Ctrl+Shift+Enter`、`Ctrl+Space` 在某些情况下到了 pwsh 就分辨不出来。Windows Terminal 用一种"win32 输入模式"把完整按键信息传给 ConPTY，PSReadLine 就能收到准确的按键。

- **M1**：先用 xterm.js 标准的 VT 编码，保证兼容。
- **M2**：做一个实验开关，自己编码 win32 输入模式，验证 PSReadLine 和 ssh、vim 都正常后再决定是否默认打开。**【待核实】**

## 2. 输出管道

```
ConPTY 输出 ──读线程──► 缓冲区 ──每 4ms 或满 64KB──► Tauri Channel ──► xterm.write()
```

- 每个会话一个读线程，阻塞读 ConPTY 输出管道。
- 攒批发送：4ms 或 64KB，先到哪个算哪个。这样 `cat` 大文件时不会把 IPC 挤爆，平时打字又几乎没有延迟。
- **背压**：xterm.js `write` 回调告诉我们它处理完了。如果积压超过 2MB，Rust 端暂停读取，让 pwsh 自然等待，避免界面卡死。
- Rust 端在转发前扫描 mooncli 自己的 OSC 序列（见第 4 节），拆出来变成事件，其余原样转发。

## 3. 启动 pwsh

找 pwsh 的顺序：设置里指定的路径 → `PATH` 里的 `pwsh.exe` → 常见安装位置（`%ProgramFiles%\PowerShell\7\`、Microsoft Store 版、`winget` 安装位置）。都找不到时，引导用户安装 PowerShell 7，并允许临时用 Windows PowerShell 5.1。

启动命令（参考 VS Code 的做法）：

```text
pwsh.exe -NoLogo -NoExit -Command "try { . '<mooncli>\shell\pwsh\MoonCli.ps1' } catch {}"
```

- 用户的 `$PROFILE` 照常先加载，所以 oh-my-posh、starship 这些提示符美化都还在。
- 集成脚本在 profile **之后**运行，包住用户已经设置好的 prompt。
- `try/catch` 保证集成脚本出错也不影响正常使用（原则 5：出问题能退回去）。
- 设置里可以关掉 shell 集成；关掉后智能功能降级，终端照常可用。

## 4. Shell 集成协议

shell 集成脚本会在 pwsh 的输出里夹带一些"看不见的标记"（OSC 序列），告诉 mooncli：提示符从哪开始、命令什么时候开始和结束、当前目录在哪。

### 用到的标准序列

| 序列 | 含义 | 出处 |
| --- | --- | --- |
| `OSC 133 ; A` | 提示符开始 | FinalTerm 标准，Windows Terminal、WezTerm、iTerm2 都支持 |
| `OSC 133 ; B` | 提示符结束，输入开始 | 同上 |
| `OSC 133 ; C` | 命令开始执行 | 同上 |
| `OSC 133 ; D ; <退出码>` | 命令结束 | 同上 |
| `OSC 7 ; file://<主机>/<路径>` | 当前目录 | 通用标准 |
| `OSC 9 ; 9 ; <路径>` | 当前目录 | Windows Terminal 的写法，一并支持 |
| `OSC 0 / OSC 2 ; <标题>` | 设置窗口/标签标题 | 通用标准，Claude Code 等程序会用 |
| `OSC 9 ; <文字>`、`OSC 777 ; notify ; …` | 桌面通知 | 一些程序会发 |

用标准序列的好处：远程服务器上的 bash/zsh 只要装了任何一种常见的 shell 集成（比如 WezTerm 的脚本），mooncli 也能识别。

### mooncli 私有序列

标准里没有的信息用私有序列，格式：

```text
ESC ] 6973 ; <nonce> ; <类型> ; <base64 编码的 JSON> BEL
```

| 类型 | 内容 |
| --- | --- |
| `cmd` | 即将执行的完整命令文本（来自 PSReadLine，多行命令也完整） |
| `env` | pwsh 版本、主机名、是否管理员 |
| `completions` | 补全请求的结果（见 [04](04-command-memory.md)） |
| `buffer` | 当前输入行内容和光标位置（只在常驻补全面板打开时发） |

**nonce 是干什么的**：mooncli 启动每个会话时生成一个随机串，通过环境变量交给集成脚本。只有带对 nonce 的私有序列才会被当真。这样 ssh 到远程服务器后，远程程序就算打印出长得一样的序列，也骗不了 mooncli（VS Code 也是这么防的）。

### 集成脚本要点（草稿）

```powershell
# MoonCli.ps1 —— 草稿，示意关键结构
$script:Nonce = $env:MOONCLI_NONCE
$script:OrigPrompt = $function:prompt
$script:LastHistoryId = -1

function script:Osc([string]$body) { "`e]$body`a" }
function script:Priv([string]$type, $obj) {
    $json = $obj | ConvertTo-Json -Compress -Depth 5
    $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    Osc "6973;$script:Nonce;$type;$b64"
}

function global:prompt {
    # 先算上一条命令的退出码（必须在调用原 prompt 之前取）
    $ok = $?
    $code = if ($global:LASTEXITCODE -ne $null -and -not $ok) { $global:LASTEXITCODE } elseif ($ok) { 0 } else { 1 }
    $out = ""
    $last = Get-History -Count 1
    if ($last -and $last.Id -ne $script:LastHistoryId) {
        $out += Osc "133;D;$code"
        $script:LastHistoryId = $last.Id
    }
    $loc = $executionContext.SessionState.Path.CurrentLocation
    if ($loc.Provider.Name -eq 'FileSystem') {
        $out += Osc "9;9;$($loc.ProviderPath)"
        $out += Osc "7;file://$env:COMPUTERNAME/$($loc.ProviderPath -replace '\\','/')"
    }
    $out += Osc "133;A"
    $out += & $script:OrigPrompt
    $out += Osc "133;B"
    $out
}

# 包住 PSReadLine 的读行函数，拿到用户真正提交的命令文本
$script:OrigReadLine = $function:PSConsoleHostReadLine
function global:PSConsoleHostReadLine {
    $line = & $script:OrigReadLine
    [Console]::Write((Priv 'cmd' @{ text = $line }) + (Osc "133;C"))
    $line
}
```

实现时要处理的细节：

- 用户的 prompt 可能是 oh-my-posh 生成的，返回值里本身就带颜色序列，直接拼接即可。
- PSReadLine 没加载（比如用户禁用了）时，`PSConsoleHostReadLine` 不存在，要降级为只发 133 A/B/D。
- 退出码判断的写法要对 cmdlet 失败、原生程序非 0 退出、`Ctrl+C` 中断三种情况分别测试。**【待核实】**

## 5. 渲染

| 项 | 做法 |
| --- | --- |
| 渲染器 | xterm.js WebGL 插件。WebGL 上下文丢失（显卡驱动重置、远程桌面切换）时自动退回 DOM 渲染器，并在空闲时尝试恢复 |
| 字符宽度 | 加载 `unicode11` 插件，中文、emoji 宽度正确 |
| 回滚行数 | 默认 10000 行，可调；超过 10 万行给提示（内存） |
| 浅色可读性 | 打开 xterm.js 的 `minimumContrastRatio`（默认 4.5），程序输出了浅色背景上看不清的颜色时自动加深 |
| 图片 | 可选加载 `image` 插件，支持 Sixel 和 iTerm2 图片协议 |
| 链接 | `web-links` 插件 + 自定义识别文件路径（`src/main.rs:42` 这种），Ctrl+单击打开 |

## 6. 会话生命周期

| 事件 | 处理 |
| --- | --- |
| 新建 | 创建 ConPTY → 启动 pwsh → 等第一个 `133;A`（最多 5 秒；超时就当作没有集成继续用） |
| pwsh 退出 | 标签页显示"已退出（代码 N）"，按回车重启、按 Esc 关闭；可设置为自动关闭 |
| 关闭标签页 | 有前台程序在运行（比如 claude、ssh）时先确认 |
| 关闭窗口 | 可选"恢复上次会话"：记住每个标签页的目录、标题、布局，下次打开按原样重建（不恢复进程本身） |
