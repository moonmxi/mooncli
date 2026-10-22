# 02 终端内核

这一篇讲"终端本身"怎么工作：怎么启动 pwsh、输入输出怎么走、mooncli 怎么知道 pwsh 里发生了什么。

## 1. ConPTY

ConPTY 是 Windows 提供的"伪终端"：我们创建一个，它会给 pwsh 一个假的控制台，然后把 pwsh 的输出翻译成标准的终端字符序列（VT 序列）给我们，我们再交给 xterm.js 显示。

### 用自带的新版 ConPTY

系统自带的 ConPTY 跟着 Windows 版本走，老系统上有不少已修复的 bug（重绘错乱、改尺寸丢内容等）。Windows Terminal 和 WezTerm 的做法是**随程序附带新版 `conpty.dll` 和 `OpenConsole.exe`**，我们照做：

- 构建时从微软官方的 `Microsoft.Windows.Console.ConPTY` NuGet 包取这两个文件，放在资源目录的 `conpty/` 子目录里。
- 我们用的 `portable-pty`（WezTerm 的伪终端库）第一次创建伪终端时，会按系统的 DLL 搜索顺序找 `conpty.dll`，找不到就用系统 `kernel32` 里的版本。所以启动时用 `SetDllDirectoryW` 把 `conpty/` 加进搜索路径即可（`app/src-tauri/src/conpty.rs`）。
- 设置里留一个开关"使用附带的 ConPTY"，关掉时不加这个目录，就会退回系统版本，方便排查问题。

**这一条不是锦上添花，是必须的。** 第二版安装包在用户机器上仍然显示"未集成"，而 CI 里的 Windows 测试却是通过的。差别在于：CI 用的是最新的 Windows Server，自带的 ConPTY 会把 shell 集成标记原样转发；而 Windows 10 和较早的 Windows 11 自带的 ConPTY 会把它不认识的终端标记（OSC 133、OSC 7、mooncli 的私有序列）吞掉，终端一个信号都收不到。现在：

- 打包前由 `tools/fetch_conpty.py` 下载微软官方 `Microsoft.Windows.Console.ConPTY` 包（固定版本、校验 SHA-256），取出 x64 的 `conpty.dll` 和 `OpenConsole.exe` 放进安装包。
- Windows CI 会先用系统自带的、再用附带的 ConPTY 各跑一遍 pwsh 集成测试。
- 设置 → Shell 与诊断里显示系统版本、用的是哪个 ConPTY，以及当前标签页**收到了多少集成标记**（本机标记、其它标记、私有消息、输出字节数）。据此自动判断"未集成"的原因：一个标记都没收到 + 系统 ConPTY → ConPTY 吞了标记；只收到不带 nonce 的标记 → 是里面又开了别的 shell；等等。

### 创建参数

| 项 | 做法 |
| --- | --- |
| 尺寸 | 用 xterm.js 算出的行列数创建；窗口改尺寸时防抖 50ms 再调 `ResizePseudoConsole` |
| 标志位 | `portable-pty` 固定打开 `PSEUDOCONSOLE_RESIZE_QUIRK`（避免改尺寸时重复绘制）和 `PSEUDOCONSOLE_WIN32_INPUT_MODE`，见下文 |
| 环境变量 | 继承用户环境，另加 `TERM_PROGRAM=mooncli`、`TERM_PROGRAM_VERSION`、`MOONCLI_SESSION_ID`、`COLORTERM=truecolor` |
| 工作目录 | 新标签页默认用"当前标签页所在目录"，可在设置里改成固定目录或用户主目录 |

### 键盘编码（WIN32_INPUT_MODE）

VT 序列表达不了所有按键组合，比如 `Ctrl+Shift+Enter`、`Ctrl+Space` 在某些情况下到了 pwsh 就分辨不出来。Windows Terminal 用一种"win32 输入模式"把完整按键信息传给 ConPTY，PSReadLine 就能收到准确的按键。

`portable-pty` 创建 ConPTY 时已经带了 `WIN32_INPUT_MODE` 标志，效果是 ConPTY 启动后会向终端发 `CSI ? 9001 h`，**请求**终端改用这种编码；终端不回应，就继续用普通 VT 输入。xterm.js 不认识这个请求，所以现在用的是普通 VT 输入，一切正常。

结论：用 xterm.js 标准的 VT 编码，不做 win32 输入模式。PSReadLine、ssh、vim 在普通 VT 输入下都正常；mooncli 自己要用的组合键（补全面板的 `Alt+/` 等）在 xterm.js 这一层就截住了，不需要传到 pwsh。

### 光标位置查询

pwsh（.NET 控制台）和 PSReadLine 会发 `ESC [ 6 n` 问终端"光标在哪"，并**等待回答**。没人回答时 .NET 的控制台读取会出错，读到的命令变成乱码（集成测试里实际遇到过）。xterm.js 会自动回答，所以界面里没问题；但任何不经过 xterm.js、直接驱动伪终端的代码（测试、以后的无界面模式）都必须自己回答。

## 2. 输出管道

```
ConPTY 输出 ──读线程──► 缓冲区 ──每 4ms 或满 64KB──► Tauri Channel ──► xterm.write()
```

- 每个会话一个读线程，阻塞读 ConPTY 输出管道。
- 攒批发送：4ms 或 64KB，先到哪个算哪个。这样 `cat` 大文件时不会把 IPC 挤爆，平时打字又几乎没有延迟。
- **背压**：xterm.js `write` 回调告诉我们它处理完了。如果积压超过 2MB，Rust 端暂停读取，让 pwsh 自然等待，避免界面卡死。
- Rust 端在转发前扫描 mooncli 自己的 OSC 序列（见第 4 节），拆出来变成事件，其余原样转发。

## 3. 启动 pwsh

找 pwsh 的顺序：设置里指定的路径 → `PATH` 里的 `pwsh.exe` → 常见安装位置（`%ProgramFiles%\PowerShell\7\`、Microsoft Store 版、`winget` 安装位置）。都找不到时见下面的 §3.1。

### 3.1 电脑上没有 PowerShell 7

不崩溃，也**不悄悄换成** Windows 自带的 Windows PowerShell 5.1（那样用户只会看到"未集成"，不知道为什么）。标签页里显示一页说明（`app/src/components/ShellMissing.tsx`），给四条路，由用户选：

| 选项 | 做什么 |
| --- | --- |
| **一键安装（推荐）** | 开一个标签页，用 Windows PowerShell 5.1 运行 `winget install --id Microsoft.PowerShell --exact --source winget`，过程全部显示出来。winget 结束后重新检测，找到 pwsh 7 就提示"装好了"并自动打开一个 pwsh 7 标签页；装失败时显示原因和官网下载地址。电脑上没有 winget（老版本 Windows 10）时不显示这一项 |
| 去微软官网下载 | 用浏览器打开 `https://aka.ms/PSWindows`，自己装 .msi，装好后点"重新检测" |
| 先用 Windows PowerShell 5.1 | 马上能用；集成脚本在 5.1 上也会尝试加载（脚本里 pwsh 7 才有的东西都做了判断），但功能可能不完整。状态栏显示"Windows PowerShell 5.1（临时）" |
| 已经装了，指定位置 | 打开设置 → Shell，填 pwsh.exe 的路径 |

- 底部的"重新检测"随时可点：检测时直接看 `%ProgramFiles%\PowerShell\7\pwsh.exe` 等固定位置，所以刚装好、PATH 还没刷新也找得到。
- 设置里指定的路径没有文件时，说明里会额外提一句。
- Rust 端：`find_shell` 只认 pwsh 7；`shell_status` 命令重新检测并报告 5.1 和 winget 在不在；`session_create` 找不到时返回以 `NO_PWSH` 开头的错误，界面据此显示说明页。

启动命令：

```text
pwsh.exe -NoLogo -NoExit -EncodedCommand <base64>
```

`<base64>` 解码后是：

```powershell
try { $null = New-Module -Name MoonCli -ScriptBlock { <MoonCli.ps1 的全文> } }
catch { Write-Host "mooncli: shell 集成没有加载成功（原因）。终端照常可用，只是没有智能功能。" -ForegroundColor DarkYellow }
```

- 集成脚本**编进程序里**（`crates/mooncli-pty/src/shell.rs` 的 `INTEGRATION_SCRIPT`），不依赖安装目录里的文件。
- 用户的 `$PROFILE` 照常先加载，所以 oh-my-posh、starship 这些提示符美化都还在；集成脚本在 profile **之后**运行，包住用户已经设置好的 prompt。
- 加载失败时打印一行黄色提示，终端照常可用（原则 5：出问题能退回去）。
- 设置里可以关掉 shell 集成；关掉后智能功能降级，终端照常可用。

**为什么不用 `-Command ". 'MoonCli.ps1'"` 点加载脚本文件**：第一版这么做，结果在 Windows 真机上显示"未集成"。点加载有三个隐患：

1. 脚本文件受用户的执行策略限制，有些机器上会被直接拦下；字符串形式的脚本块不受这个限制。
2. 安装目录的路径在 Windows 上可能带 `\\?\` 前缀、空格、引号，拼进命令行容易出错。
3. 外面那层 `try/catch {}` 把错误吞掉了，用户和我们都看不到原因。

另外，脚本放进**动态模块**里运行，内部的变量和辅助函数（`$Nonce`、`Osc` 等）不会漏进用户的全局作用域。集成测试里专门检查了这一点。

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

**nonce 是干什么的**：mooncli 启动每个会话时生成一个随机串，通过环境变量 `MOONCLI_NONCE` 交给集成脚本。只有带对 nonce 的私有序列才会被当真。这样 ssh 到远程服务器后，远程程序就算打印出长得一样的序列，也骗不了 mooncli（VS Code 也是这么防的）。

nonce 还用在标准的 133 标记上：本机集成脚本发的是 `OSC 133 ; A ; nonce=<随机串>`（133 标记允许带 `键=值` 选项，其它终端会忽略不认识的选项）。mooncli 只有收到带对 nonce 的标记，才会认为"本机 pwsh 在等待输入"并开始处理补全类按键；远程 shell 发的 133 标记不带 nonce，只用来记录远程信息（见 [05](05-interaction.md) §1）。

集成脚本加载后会立刻从环境变量里删掉 `MOONCLI_NONCE`，所以从这个 pwsh 启动的子进程（ssh、claude、嵌套的 pwsh）都拿不到它。

### 集成脚本

正式版在 [`shell/pwsh/MoonCli.ps1`](../shell/pwsh/MoonCli.ps1)，由 `crates/mooncli-pty/tests/pwsh_integration.rs` 在真实 pwsh 里逐项测试（提示符标记、命令文本、四种退出码情况、空回车、切换目录、多行命令、补全请求）。下面是最初的设计草稿，保留作说明：

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

实现时处理的细节：

- 用户的 prompt 可能是 oh-my-posh 生成的，返回值里本身就带颜色序列，直接拼接即可。
- PSReadLine 没加载时，先尝试 `Import-Module PSReadLine`；还是没有 `PSConsoleHostReadLine` 就降级为只发 133 A/B/D。
- 退出码：cmdlet 失败时 `$LASTEXITCODE` 可能还是上一次原生程序留下的值，直接用会误报。正式版在命令开始时记下 `$LASTEXITCODE`，结束时只有它变了才采用，否则报 1。已在测试里覆盖。
- 空回车不发"命令开始/结束"标记，不算一条命令。
- `Ctrl+C` 中断时的退出码还没有专门测试。**【待核实】**

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
