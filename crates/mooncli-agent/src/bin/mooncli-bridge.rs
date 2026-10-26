//! mooncli-bridge：Claude Code 与 Codex 的官方扩展点都指向这个小工具（docs/07 第 3 节）。
//!
//! - `mooncli-bridge claude-statusline`：作 Claude Code 的 statusLine 命令。读 stdin 的 JSON，
//!   写进 spool 给主进程；用户原来的状态栏命令若被接入流程保存过，原样转调并转发它的输出，
//!   没有就输出自带的一行（模型 · 上下文 · 额度）。
//! - `mooncli-bridge claude-hook`：作 hooks 命令。读 stdin 的事件 JSON，写进 spool；
//!   一轮结束（Stop）、要你确认（Notification）时发完成提醒（见 mooncli_agent::notify）。
//! - `mooncli-bridge codex-notify`：作 Codex 的 notify。最后一个 argv 是事件 JSON
//!   （目前只有 agent-turn-complete），写进 spool 并发完成提醒；用户原来的 notify 若保存过，同样转调（不等待）。
//! - `mooncli-bridge test-toast`：弹一条测试通知（设置页的「试一下」用不到它，留给命令行排查）。
//!
//! 铁律：永远以 0 退出、不往 stdout 输出多余内容（statusline 的那一行除外）、
//! 任何错误都吞掉——绝不能因为 mooncli 的问题影响 Claude Code / Codex 本身。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mooncli_agent::notify::{self, Alert};
use mooncli_agent::spool::{
    SpoolEntry, claude_hook_file_name, claude_status_file_name, codex_notify_file_name, write_entry,
};
use serde_json::Value;

fn agent_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOONCLI_AGENT_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    default_agent_dir()
}

/// 在 mooncli 外面启动的 Claude Code / Codex 没有 MOONCLI_AGENT_DIR，按 mooncli 的数据目录约定找
/// （Tauri 的 app_local_data_dir：%LOCALAPPDATA%\dev.moonmxi.mooncli）。
fn default_agent_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("dev.moonmxi.mooncli").join("agent")
}

fn tab_session_id() -> Option<String> {
    std::env::var("MOONCLI_SESSION_ID").ok().filter(|s| !s.trim().is_empty())
}

fn bridge_config(dir: &Path) -> Value {
    std::fs::read_to_string(dir.join("bridge-config.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

fn read_stdin() -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut buf);
    buf
}

fn spool(dir: &Path, file_name: &str, payload: Value) {
    let entry = SpoolEntry { received_at: mooncli_agent::now_epoch(), tab_session_id: tab_session_id(), payload };
    let _ = write_entry(dir, file_name, &entry);
}

/// Claude Code 在 Windows 上用 Git Bash 跑 statusLine 命令，转调用户原命令时也要用 bash，
/// 否则 `~/.claude/statusline.sh` 这类写法跑不起来。按 PATH 找 bash.exe，跳过 System32 里
/// WSL 的 bash（那是另一个系统）；找不到再退回 cmd。
fn shell_for(command: &str) -> Command {
    if cfg!(windows) {
        let bash =
            std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from).filter(|p| p.is_file()).or_else(|| {
                let path = std::env::var_os("PATH")?;
                std::env::split_paths(&path)
                    .filter(|d| !d.to_string_lossy().to_lowercase().contains("system32"))
                    .map(|d| d.join("bash.exe"))
                    .find(|p| p.is_file())
            });
        match bash {
            Some(b) => {
                let mut c = Command::new(b);
                c.args(["-c", command]);
                c
            }
            None => {
                let mut c = Command::new("cmd");
                c.args(["/C", command]);
                c
            }
        }
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    }
}

/// 跑一条命令字符串，喂给它 stdin，返回它的 stdout。
fn run_shell_command(command: &str, stdin_data: &[u8]) -> Option<Vec<u8>> {
    let mut cmd = shell_for(command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data);
    }
    let output = child.wait_with_output().ok()?;
    (output.status.success() || !output.stdout.is_empty()).then_some(output.stdout)
}

/// 没有原状态栏命令时输出的一行：`Opus · 上下文 8% · 5 小时 24% · 本周 41%`。
fn builtin_line(p: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(m) = p.pointer("/model/display_name").and_then(Value::as_str) {
        parts.push(m.to_string());
    }
    if let Some(pct) = p.pointer("/context_window/used_percentage").and_then(Value::as_f64) {
        parts.push(format!("上下文 {}%", pct.round()));
    }
    for (key, label) in [("five_hour", "5 小时"), ("seven_day", "本周")] {
        if let Some(pct) = p.pointer(&format!("/rate_limits/{key}/used_percentage")).and_then(Value::as_f64) {
            parts.push(format!("{label} {}%", pct.round()));
        }
    }
    parts.join(" · ")
}

fn claude_statusline() {
    let raw = read_stdin();
    let Ok(payload) = serde_json::from_slice::<Value>(&raw) else { return };
    let dir = agent_dir();
    let config = bridge_config(&dir);
    if let Some(id) = payload.get("session_id").and_then(Value::as_str) {
        spool(&dir, &claude_status_file_name(id), payload.clone());
    }
    let previous =
        config.pointer("/previousClaudeStatusLine/command").and_then(Value::as_str).filter(|s| !s.trim().is_empty());
    if let Some(prev) = previous {
        if let Some(out) = run_shell_command(prev, &raw) {
            let _ = std::io::stdout().write_all(&out);
        }
        return;
    }
    if config.get("builtinStatusLine").and_then(Value::as_bool) != Some(false) {
        let line = builtin_line(&payload);
        if !line.is_empty() {
            println!("{line}");
        }
    }
}

/// hooks 载荷里只留状态要用的几个字段：PostToolUse 之类会带上整段工具输出（几十 KB），
/// 每次都写盘没有意义，也不该把这些内容留在 mooncli 的目录里。
const HOOK_FIELDS: [&str; 6] =
    ["session_id", "hook_event_name", "cwd", "transcript_path", "notification_type", "message"];

fn claude_hook() {
    let raw = read_stdin();
    let Ok(payload) = serde_json::from_slice::<Value>(&raw) else { return };
    let Some(id) = payload.get("session_id").and_then(Value::as_str).map(str::to_string) else { return };
    let dir = agent_dir();
    let slim: serde_json::Map<String, Value> =
        HOOK_FIELDS.iter().filter_map(|k| payload.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
    spool(&dir, &claude_hook_file_name(&id), Value::Object(slim));
    let transcript = payload.get("transcript_path").and_then(Value::as_str).map(PathBuf::from);
    let last_text = || transcript.and_then(|t| mooncli_agent::claude::transcript_tail(&t).last_text);
    if let Some(alert) = notify::from_claude_hook(&payload, last_text, mooncli_agent::now_epoch()) {
        deliver(&dir, alert);
    }
}

/// 发一条提醒：交给 mooncli 主程序（它知道你是不是正看着那个标签页，决定弹不弹）。
/// 不在 mooncli 的标签页里、或者设置里关了提醒，就什么都不做。
fn deliver(dir: &Path, mut alert: Alert) {
    let prefs = bridge_config(dir).get("notify").cloned().unwrap_or(Value::Null);
    if prefs.get("enabled").and_then(Value::as_bool) == Some(false) {
        return;
    }
    // 只提醒跑在 mooncli 标签页里的 Agent。Claude Code 的 hooks、Codex 的 notify 是全局配置，
    // Claude 桌面版、IDE 插件、别的终端里的会话也会调到这里，它们一律不提醒
    let Some(tab) = tab_session_id() else { return };
    if !notify::ancestors().iter().any(|(_, name)| name == "mooncli.exe") {
        return;
    }
    alert.tab_session_id = Some(tab);
    let _ = notify::write_alert(dir, &alert);
}

/// 自己弹通知。先确认通知身份登记过：没登记的话 Windows 会悄悄丢掉通知，
/// 而 mooncli 主程序不一定启动过（图标由主程序放进 agent 目录，没有就不带图标）。
fn toast(dir: &Path, title: &str, body: &str) -> bool {
    let icon = dir.join("mooncli.png");
    let _ = notify::register_app_id(icon.is_file().then_some(icon.as_path()));
    match notify::show_toast(title, body, None) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("{e}");
            false
        }
    }
}

fn codex_notify(args: &[String]) {
    // Codex 把事件 JSON 作为最后一个参数传进来
    let Some(json_text) = args.last() else { return };
    let Ok(payload) = serde_json::from_str::<Value>(json_text) else { return };
    let dir = agent_dir();
    if let Some(thread_id) = payload.get("thread-id").and_then(Value::as_str) {
        // 只留关联会话要用的字段，用户的提问和 Agent 的回复不落进 mooncli 的目录
        let slim: serde_json::Map<String, Value> = ["type", "thread-id", "turn-id", "cwd"]
            .iter()
            .filter_map(|k| payload.get(*k).map(|v| (k.to_string(), v.clone())))
            .collect();
        spool(&dir, &codex_notify_file_name(thread_id), Value::Object(slim));
    }
    if let Some(alert) = notify::from_codex(&payload, mooncli_agent::now_epoch()) {
        deliver(&dir, alert);
    }
    // 转调用户原来的 notify（不等待，和 Codex 自己对 notify 子进程的态度一致）
    if let Some(prev) = bridge_config(&dir).get("previousCodexNotify").and_then(Value::as_array) {
        let parts: Vec<String> = prev.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        if let Some((exe, rest)) = parts.split_first() {
            let _ = Command::new(exe)
                .args(rest)
                .arg(json_text)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("claude-statusline") => claude_statusline(),
        Some("claude-hook") => claude_hook(),
        Some("codex-notify") => codex_notify(&args[1..]),
        Some("test-toast") => {
            toast(&agent_dir(), "mooncli", "通知可以正常显示");
        }
        _ => {}
    }
    // 永远 0 退出
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_line_formats_available_parts() {
        let p = serde_json::json!({
            "model": { "display_name": "Opus" },
            "context_window": { "used_percentage": 8.4 },
            "rate_limits": { "five_hour": { "used_percentage": 23.5 } }
        });
        assert_eq!(builtin_line(&p), "Opus · 上下文 8% · 5 小时 24%");
        assert_eq!(builtin_line(&serde_json::json!({})), "");
    }
}
