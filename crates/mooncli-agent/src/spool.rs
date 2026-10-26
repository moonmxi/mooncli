//! mooncli-bridge 与主进程之间的交接目录（spool）：在 mooncli 的 agent 数据目录下，
//! bridge 把扩展点送来的数据写成小 JSON 文件，主进程只读。每个会话每种来源一个文件，只留最新一份：
//! - `claude-status-<session_id>.json`：Claude Code statusLine 的输入
//! - `claude-hook-<session_id>.json`：Claude Code hooks 的最近一个事件
//! - `codex-notify-<thread_id>.json`：Codex notify 的最近一次触发
//!
//! 每个文件的内容都是 `{ receivedAt, tabSessionId?, payload }`。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpoolEntry {
    /// bridge 写下这条数据的时刻（epoch 秒）
    pub received_at: i64,
    /// mooncli 标签页会话 ID（环境变量 MOONCLI_SESSION_ID）；在 mooncli 外运行的没有
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_session_id: Option<String>,
    /// 扩展点送来的原始 JSON
    pub payload: Value,
}

pub fn claude_status_file_name(session_id: &str) -> String {
    format!("claude-status-{}.json", sanitize(session_id))
}

pub fn claude_hook_file_name(session_id: &str) -> String {
    format!("claude-hook-{}.json", sanitize(session_id))
}

pub fn codex_notify_file_name(thread_id: &str) -> String {
    format!("codex-notify-{}.json", sanitize(thread_id))
}

/// 会话 ID 会进文件名，去掉路径分隔符等危险字符。
pub fn sanitize(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// 原子地写一个 spool 文件（先写临时文件再改名，读者不会读到一半的内容）。
pub fn write_entry(agent_dir: &Path, file_name: &str, entry: &SpoolEntry) -> std::io::Result<()> {
    std::fs::create_dir_all(agent_dir)?;
    let tmp = agent_dir.join(format!(".tmp-{}-{}", std::process::id(), file_name));
    let text = serde_json::to_string(entry)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, agent_dir.join(file_name))
}

pub fn read_entry(path: &Path) -> Option<SpoolEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// 列出 spool 目录里某前缀的文件。
pub fn list_entries(agent_dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(agent_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && n.ends_with(".json"))
        })
        .collect()
}

/// 某几类 spool 文件里最新的修改时刻（epoch 秒）：主进程据此判断 Agent 刚有动静（一轮结束、状态栏刷新），
/// 好立刻重查额度。没有文件时为 0。
pub fn latest_mtime(agent_dir: &Path, prefixes: &[&str]) -> i64 {
    prefixes
        .iter()
        .flat_map(|p| list_entries(agent_dir, p))
        .filter_map(|p| crate::timeutil::file_mtime_epoch(&p))
        .max()
        .unwrap_or(0)
}

/// 清掉很久没更新的 spool 文件（会话早就结束了），免得目录越攒越多。返回删掉的个数。
pub fn prune(agent_dir: &Path, older_than_secs: i64, now: i64) -> usize {
    let mut n = 0;
    for prefix in ["claude-status-", "claude-hook-", "codex-notify-"] {
        for path in list_entries(agent_dir, prefix) {
            let stale = crate::timeutil::file_mtime_epoch(&path).is_some_and(|t| now - t > older_than_secs);
            if stale && std::fs::remove_file(&path).is_ok() {
                n += 1;
            }
        }
    }
    n
}
