//! mooncli-bridge：Codex 的官方扩展点 notify 指向这个小工具。
//!
//! - `mooncli-bridge codex-notify`：作 Codex 的 notify。最后一个 argv 是事件 JSON
//!   （目前只有 agent-turn-complete），写进 spool 给主进程标会话状态；
//!   用户原来的 notify 若被接入流程保存过，同样转调（不等待）。
//!
//! 铁律（docs/07 第 4 节）：永远立刻以 0 退出、不往 stdout 输出任何内容、
//! 任何错误都吞掉——绝不能因为 mooncli 的问题影响 Codex 本身的行为。

use std::path::PathBuf;
use std::process::{Command, Stdio};

use mooncli_agent::spool::{SpoolEntry, codex_notify_file_name, write_entry};
use serde_json::Value;

fn agent_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOONCLI_AGENT_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
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

fn codex_notify(args: &[String]) {
    // Codex 把事件 JSON 作为最后一个参数传进来
    let Some(json_text) = args.last() else { return };
    let Ok(payload) = serde_json::from_str::<Value>(json_text) else {
        return;
    };
    let dir = agent_dir();
    if let Some(thread_id) = payload.get("thread-id").and_then(|s| s.as_str()) {
        let entry = SpoolEntry {
            received_at: mooncli_agent::now_epoch(),
            tab_session_id: tab_session_id(),
            payload: payload.clone(),
        };
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
    if args.first().map(|s| s.as_str()) == Some("codex-notify") {
        codex_notify(&args[1..]);
    }
    // 永远 0 退出
}
