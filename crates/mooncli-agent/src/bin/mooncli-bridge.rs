//! mooncli-bridge：Claude Code 与 Codex 的官方扩展点都指向这个小工具。
//!
//! - `mooncli-bridge statusline`：作 Claude Code 的 statusLine 命令。读 stdin 的 JSON，
//!   写进 spool 给主进程；用户原来的状态栏命令若被接入流程保存过，原样转调并转发其输出。
//! - `mooncli-bridge hook`：作 hooks 命令。读 stdin 的事件 JSON，写进 spool，立刻退出。
//! - `mooncli-bridge codex-notify`：作 Codex 的 notify。最后一个 argv 是事件 JSON，
//!   写进 spool；用户原来的 notify 若保存过，同样转调。
//!
//! 铁律（docs/07 第 3 节）：永远立刻以 0 退出、不往 stdout 输出多余内容（statusline 的转发除外）、
//! 任何错误都吞掉——绝不能因为 mooncli 的问题影响 Claude Code / Codex 本身的行为。

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use mooncli_agent::spool::{codex_notify_file_name, hook_file_name, status_file_name, write_entry, SpoolEntry};
use serde_json::Value;

fn agent_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOONCLI_AGENT_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    std::env::home_dir().map(|h| h.join(".mooncli").join("agent")).unwrap_or_else(|| PathBuf::from(".mooncli/agent"))
}

fn tab_session_id() -> Option<String> {
    std::env::var("MOONCLI_SESSION_ID").ok().filter(|s| !s.trim().is_empty())
}

fn bridge_config(agent_dir: &std::path::Path) -> Value {
    std::fs::read_to_string(agent_dir.join("bridge-config.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

fn now() -> i64 {
    mooncli_agent::now_epoch()
}

fn read_stdin() -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut buf);
    buf
}

/// 用系统 shell 跑一条命令字符串，喂给它 stdin，返回它的 stdout。
fn run_shell_command(command: &str, stdin_data: &[u8]) -> Option<Vec<u8>> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data);
    }
    let output = child.wait_with_output().ok()?;
    if output.status.success() || !output.stdout.is_empty() {
        Some(output.stdout)
    } else {
        None
    }
}

fn statusline() {
    let raw = read_stdin();
    let Ok(payload) = serde_json::from_slice::<Value>(&raw) else {
        return;
    };
    let dir = agent_dir();
    let config = bridge_config(&dir);

    // 1. 写 spool（只在 mooncli 里运行时才有标签页会话 ID；没有也照写，
    //    主进程按 session_id / cwd 归属，写不写由主进程的开关决定）
    if let Some(session_id) = payload.get("session_id").and_then(|s| s.as_str()) {
        let entry = SpoolEntry { received_at: now(), tab_session_id: tab_session_id(), payload: payload.clone() };
        let _ = write_entry(&dir, &status_file_name(session_id), &entry);
    }

    // 2. 转调用户原来的状态栏命令；没有则按配置输出自带的一行
    let previous = config.get("previousStatusLine").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty());
    if let Some(prev) = previous {
        if let Some(out) = run_shell_command(prev, &raw) {
            let _ = std::io::stdout().write_all(&out);
            return;
        }
    }
    if config.get("builtinStatusLine").and_then(|v| v.as_bool()) == Some(true) {
        let model = payload.get("model").and_then(|m| m.get("display_name")).and_then(|m| m.as_str()).unwrap_or("");
        let cwd = payload
            .get("workspace")
            .and_then(|w| w.get("current_dir"))
            .and_then(|c| c.as_str())
            .or_else(|| payload.get("cwd").and_then(|c| c.as_str()))
            .unwrap_or("");
        let dir_name = cwd.rsplit(['/', '\\']).next().unwrap_or(cwd);
        let mut parts = Vec::new();
        if !model.is_empty() {
            parts.push(model.to_string());
        }
        if !dir_name.is_empty() {
            parts.push(dir_name.to_string());
        }
        if !parts.is_empty() {
            println!("{}", parts.join(" · "));
        }
    }
}

fn hook() {
    let raw = read_stdin();
    let Ok(payload) = serde_json::from_slice::<Value>(&raw) else {
        return;
    };
    let session_id = payload.get("session_id").and_then(|s| s.as_str()).map(|s| s.to_string());
    if let Some(session_id) = session_id {
        let entry = SpoolEntry { received_at: now(), tab_session_id: tab_session_id(), payload };
        let _ = write_entry(&agent_dir(), &hook_file_name(&session_id), &entry);
    }
}

fn codex_notify(args: &[String]) {
    // Codex 把事件 JSON 作为最后一个参数传进来
    let Some(json_text) = args.last() else { return };
    let Ok(payload) = serde_json::from_str::<Value>(json_text) else {
        return;
    };
    let dir = agent_dir();
    if let Some(thread_id) = payload.get("thread-id").and_then(|s| s.as_str()) {
        let entry = SpoolEntry { received_at: now(), tab_session_id: tab_session_id(), payload: payload.clone() };
        let _ = write_entry(&dir, &codex_notify_file_name(thread_id), &entry);
    }
    // 转调用户原来的 notify（不等待，和 Codex 自己对 notify 子进程的态度一致）
    if let Some(prev) = bridge_config(&dir).get("previousCodexNotify").and_then(|v| v.as_array()) {
        let parts: Vec<String> = prev.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
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
    match args.first().map(|s| s.as_str()) {
        Some("statusline") => statusline(),
        Some("hook") => hook(),
        Some("codex-notify") => codex_notify(&args[1..]),
        _ => {}
    }
    // 永远 0 退出
}
