# MoonCli.ps1 —— mooncli 的 pwsh 集成脚本
#
# 本脚本编进 mooncli 程序里，启动 pwsh 时通过 -EncodedCommand 作为动态模块加载，
# 在用户 profile 之后运行。作用是在输出里夹带一些看不见的标记，告诉终端：
#   提示符从哪开始、命令什么时候开始和结束、退出码是多少、当前目录在哪。
# 协议说明见 docs/02-terminal-core.md 第 4 节。
#
# 原则：这里出任何错都不能影响 pwsh 正常使用。

if ($global:__MoonCliLoaded) { return }

$script:Nonce = $env:MOONCLI_NONCE
if (-not $script:Nonce) { return }
$global:__MoonCliLoaded = $true
# 不让子进程（ssh、claude、嵌套的 pwsh）拿到 nonce，它们发出的标记就不会被当作本机提示符
Remove-Item Env:MOONCLI_NONCE -ErrorAction SilentlyContinue

$script:E = [char]0x1b
$script:BEL = [char]0x07
$script:Running = $false
$script:ExitAtStart = $null

function script:Osc([string]$body) { "$script:E]$body$script:BEL" }

function script:Mark([string]$kind, [string]$extra = '') {
    if ($extra) { Osc "133;$kind;$extra;nonce=$script:Nonce" } else { Osc "133;$kind;nonce=$script:Nonce" }
}

function script:Priv([string]$type, $obj) {
    $json = ConvertTo-Json -InputObject $obj -Compress -Depth 6
    $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    Osc "6973;$script:Nonce;$type;$b64"
}

function script:CwdMarks {
    $loc = $ExecutionContext.SessionState.Path.CurrentLocation
    if ($loc.Provider.Name -ne 'FileSystem') { return '' }
    $p = $loc.ProviderPath
    $url = ($p -replace '\\', '/')
    if (-not $url.StartsWith('/')) { $url = "/$url" }
    # 只转义 URL 里有特殊含义的字符，中文等保持原样（终端端会做百分号解码）
    $url = $url -replace '%', '%25' -replace ' ', '%20' -replace '#', '%23' -replace '\?', '%3F'
    (Osc "9;9;`"$p`"") + (Osc "7;file://$([Environment]::MachineName)$url")
}

# ---- 提示符 ----------------------------------------------------------------

$script:OrigPrompt = $function:prompt

function global:prompt {
    # 必须最先读：任何语句都会改写 $?
    $ok = $global:?
    $native = $global:LASTEXITCODE
    $out = ''
    if ($script:Running) {
        $script:Running = $false
        $code = if ($ok) { 0 }
                elseif ($null -ne $native -and $native -ne $script:ExitAtStart -and $native -ne 0) { $native }
                else { 1 }
        $out += Mark 'D' "$code"
    }
    try { $out += CwdMarks } catch { }
    $out += Mark 'A'
    try {
        $out += & $script:OrigPrompt
    } catch {
        $out += "PS $($ExecutionContext.SessionState.Path.CurrentLocation)> "
    }
    # 原 prompt 可能改了 $LASTEXITCODE，还原给用户
    $global:LASTEXITCODE = $native
    $out + (Mark 'B')
}

# ---- 拿到用户真正提交的命令 --------------------------------------------------
# PSReadLine 把读行函数 PSConsoleHostReadLine 暴露成普通函数，包一层就能拿到完整命令文本（多行也完整）。

if (-not (Get-Command PSConsoleHostReadLine -ErrorAction SilentlyContinue)) {
    Import-Module PSReadLine -ErrorAction SilentlyContinue
}

if (Get-Command PSConsoleHostReadLine -ErrorAction SilentlyContinue) {
    $script:OrigReadLine = $function:PSConsoleHostReadLine
    function global:PSConsoleHostReadLine {
        $line = & $script:OrigReadLine
        if ($null -ne $line -and $line.Trim()) {
            $script:Running = $true
            $script:ExitAtStart = $global:LASTEXITCODE
            [Console]::Write((Priv 'cmd' @{ text = $line }) + (Mark 'C'))
        }
        $line
    }

    # ---- 补全请求（docs/04 §4）----
    # mooncli 在用户按 Ctrl+Space 时悄悄发送 F12 F11，这里算出补全结果，用私有序列送回去。
    Set-PSReadLineKeyHandler -Chord 'F12,F11' -Description 'mooncli: 把补全结果发给终端' -ScriptBlock {
        $line = $null; $cursor = $null
        [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
        $payload = @{ line = $line; cursor = $cursor; start = $cursor; len = 0; items = @() }
        try {
            $r = TabExpansion2 -inputScript $line -cursorColumn $cursor
            $payload.start = $r.ReplacementIndex
            $payload.len = $r.ReplacementLength
            $payload.items = @($r.CompletionMatches | Select-Object -First 300 | ForEach-Object {
                @{ t = $_.CompletionText; l = $_.ListItemText; k = "$($_.ResultType)"; d = $_.ToolTip }
            })
        } catch { }
        [Console]::Write((Priv 'completions' $payload))
    }
}

# ---- 配色适配（docs/08 §2） ------------------------------------------------------
# PSReadLine 的默认配色是按深色终端设计的：普通文字用"白色"、数字用"亮白"、参数用"亮黑"，
# 而灰色的联想提示用"亮白 + 变暗 + 斜体"。在浅色主题里这几种都是灰色，输入的文字和联想提示分不清。
# 这里把它们换成跟随主题的颜色。只替换仍是默认值的项，用户自己设过的不动。
# 用的都是 ANSI 色号，所以切换主题时终端会自动换成新主题的对应颜色。

if ($env:MOONCLI_ADAPT_COLORS -eq '1') {
    try {
        $e = [char]0x1b
        $opt = Get-PSReadLineOption -ErrorAction Stop
        # 键：Set-PSReadLineOption -Colors 用的颜色名；值：(默认值, 新值)
        # 注意普通文字的颜色名是 Default，读取时的属性名却是 DefaultTokenColor
        $map = [ordered]@{
            Default                     = "$e[37m",       "$e[39m"      # 普通文字：终端前景色
            ContinuationPrompt          = "$e[37m",       "$e[90m"
            Command                     = "$e[93m",       "$e[34m"      # 命令：蓝
            Parameter                   = "$e[90m",       "$e[36m"      # 参数：青（原来是和联想提示差不多的灰）
            Operator                    = "$e[90m",       "$e[39m"
            String                      = "$e[36m",       "$e[32m"      # 字符串：绿
            Variable                    = "$e[92m",       "$e[35m"      # 变量：品红
            Keyword                     = "$e[92m",       "$e[35;1m"
            Number                      = "$e[97m",       "$e[33m"      # 数字：琥珀（原来是和联想提示同色的亮白）
            Type                        = "$e[37m",       "$e[36m"
            Member                      = "$e[37m",       "$e[39m"
            Selection                   = "$e[30;47m",    "$e[7m"       # 选中：反色（原来的黑字白底在浅色主题里是深灰底）
            ListPredictionSelected      = "$e[48;5;238m", "$e[7m"       # 列表联想的选中行：同上
            # 联想提示：亮黑 + 斜体。mooncli 的主题里亮黑在浅色下是约 3:1 的灰、在深色下是约 4:1 的暗灰，
            # 两边都明显比正文淡又能看清。默认的"亮白 + 变暗"在浅色下几乎看不见，在深色下亮白又是最亮的颜色
            InlinePrediction            = "$e[97;2;3m",   "$e[90;3m"
            ListPredictionTooltip       = "$e[97;2;3m",   "$e[90;3m"
        }
        foreach ($k in $map.Keys) {
            $prop = if ($k -eq 'Default') { 'DefaultTokenColor' } else { "$($k)Color" }
            # 每项单独设置：某个颜色名在旧版 PSReadLine 里不存在时，只跳过这一项
            try {
                if ($opt.$prop -eq $map[$k][0]) { Set-PSReadLineOption -Colors @{ $k = $map[$k][1] } -ErrorAction Stop }
            } catch { }
        }
        # PowerShell 7.2+ 默认用蓝底显示目录名，在浅色背景上很扎眼，改成蓝色粗体字
        if ($PSStyle -and $PSStyle.FileInfo.Directory -eq "$e[44;1m") { $PSStyle.FileInfo.Directory = "$e[34;1m" }
    } catch { }
}

# ---- 会话信息 ----------------------------------------------------------------

try {
    $admin = $false
    if ($IsWindows -or $PSVersionTable.PSEdition -eq 'Desktop') {
        $id = [Security.Principal.WindowsIdentity]::GetCurrent()
        $admin = ([Security.Principal.WindowsPrincipal]$id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    }
    [Console]::Write((Priv 'env' @{
        pwsh = $PSVersionTable.PSVersion.ToString()
        host = [Environment]::MachineName
        admin = $admin
        psreadline = [bool](Get-Command PSConsoleHostReadLine -ErrorAction SilentlyContinue)
    }))
} catch { }

# 本脚本作为动态模块加载（见 crates/mooncli-pty/src/shell.rs 的 integration_bootstrap）。
# 只有上面显式写了 global: 的 prompt、PSConsoleHostReadLine 会出现在用户会话里，辅助函数和变量都不导出。
Export-ModuleMember -Function @() -Variable @()
