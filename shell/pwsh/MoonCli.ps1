# MoonCli.ps1 —— mooncli 的 pwsh 集成脚本
#
# mooncli 启动 pwsh 时用 -NoExit -Command ". 'MoonCli.ps1'" 加载本脚本，
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
