//! Claude Code 的数据（docs/07 第 2 节）：
//! 1. statusLine 输入（官方扩展点）：mooncli-bridge 作为 statusLine 命令，把 Claude Code 送来的
//!    JSON 写进 spool。里面有上下文占用（context_window）和账号额度（rate_limits 的
//!    five_hour / seven_day，claude.ai Pro/Max 订阅、首次 API 响应之后才有）。
//! 2. hooks（官方扩展点）：会话状态——提交提示词后在跑、要你确认、一轮结束。
//! 3. 会话转录 `~/.claude/projects/<项目>/<会话>.jsonl`（Claude Code 自己写的文件）：
//!    没接入 statusLine 时，用最后一条回复的 usage 补上下文占用。额度只来自 statusLine，不估算。
//!
//! 字段以官方文档（code.claude.com/docs/en/statusline、hooks）为准，2026-10 核实；
//! 每个字段都「不存在就降级」，新旧版本字段有出入时不至于整体失效。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonl::{collect_jsonl, tail_lines};
use crate::model::{AccountUsage, AgentKind, ContextUsage, RateWindow, SessionState, SessionUsage, SnapshotSource};
use crate::spool::{SpoolEntry, claude_hook_file_name, list_entries, read_entry};
use crate::timeutil::file_mtime_epoch;

/// 最多看多少个最近的转录文件。
const MAX_TRANSCRIPTS: usize = 40;
/// 小窗最多列多少个 Claude Code 会话。
const MAX_SESSIONS: usize = 20;
/// hooks 状态多久没更新就算过期（秒）
const STALE_STATE_SECS: i64 = 30 * 60;

/// Claude Code 配置目录：CLAUDE_CONFIG_DIR 优先，否则 ~/.claude。
pub fn claude_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    std::env::home_dir().map(|h| h.join(".claude")).unwrap_or_default()
}

/// 只有转录可看时的上下文上限：官方默认 20 万，扩展上下文的模型 100 万。转录里没写上限，
/// 模型 ID 带 `[1m]`、或者实际用量已经超过 20 万的，就是 100 万的那种。
/// 接入 statusLine 后用它给的 context_window_size（权威值），不走这里。
fn context_limit(model: &str, used: i64) -> i64 {
    if model.contains("[1m]") || used > 200_000 { 1_000_000 } else { 200_000 }
}

/// 输入侧 token：input + cache_creation + cache_read（与官方 used_percentage 的口径一致，不含 output）。
fn usage_context_tokens(usage: &Value) -> i64 {
    let get = |k: &str| usage.get(k).and_then(Value::as_i64).unwrap_or(0);
    get("input_tokens") + get("cache_read_input_tokens") + get("cache_creation_input_tokens")
}

// ---------- 转录 ----------

pub struct TranscriptInfo {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub context: Option<ContextUsage>,
}

/// 尾部扫描一个转录文件：最后一条真实回复（不是 `<synthetic>`）的 usage → 上下文占用。
pub fn transcript_tail(path: &Path) -> TranscriptInfo {
    let mut info = TranscriptInfo { session_id: None, cwd: None, model: None, context: None };
    let mut tail = 256 * 1024u64;
    loop {
        for line in tail_lines(path, tail).iter().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            if info.session_id.is_none() {
                info.session_id = v.get("sessionId").and_then(Value::as_str).map(str::to_string);
            }
            if info.cwd.is_none() {
                info.cwd = v.get("cwd").and_then(Value::as_str).map(str::to_string);
            }
            if info.context.is_none()
                && v.get("type").and_then(Value::as_str) == Some("assistant")
                && let Some(msg) = v.get("message")
            {
                let model = msg.get("model").and_then(Value::as_str).unwrap_or("");
                if !model.is_empty()
                    && model != "<synthetic>"
                    && let Some(usage) = msg.get("usage")
                {
                    let used = usage_context_tokens(usage);
                    if used > 0 {
                        info.model = Some(model.to_string());
                        info.context =
                            Some(ContextUsage { used_tokens: used, window_tokens: context_limit(model, used) });
                    }
                }
            }
            if info.session_id.is_some() && info.cwd.is_some() && info.context.is_some() {
                return info;
            }
        }
        if info.context.is_some() || tail >= 4 * 1024 * 1024 {
            return info;
        }
        tail *= 4;
    }
}

// ---------- statusLine ----------

pub struct StatuslineData {
    pub session_id: String,
    pub tab_session_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub context: Option<ContextUsage>,
    pub transcript_path: Option<String>,
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
    pub received_at: i64,
}

/// resets_at 官方是 epoch 秒；保险起见 RFC3339 字符串也认。
fn resets_at_value(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_f64().map(|f| f as i64))
        .or_else(|| v.as_str().and_then(crate::timeutil::parse_rfc3339_epoch))
}

fn statusline_window(v: &Value, window_minutes: i64) -> Option<RateWindow> {
    Some(RateWindow {
        used_percent: v.get("used_percentage")?.as_f64()?,
        window_minutes: Some(window_minutes),
        resets_at: v.get("resets_at").and_then(resets_at_value),
        estimated: false,
    })
}

pub fn parse_statusline(entry: &SpoolEntry) -> Option<StatuslineData> {
    let p = &entry.payload;
    let session_id = p.get("session_id")?.as_str()?.to_string();
    let model_obj = p.get("model");
    let model_id = model_obj.and_then(|m| m.get("id")).and_then(Value::as_str);
    let model = model_obj.and_then(|m| m.get("display_name")).and_then(Value::as_str).or(model_id).map(str::to_string);
    let cwd = p
        .get("workspace")
        .and_then(|w| w.get("current_dir"))
        .and_then(Value::as_str)
        .or_else(|| p.get("cwd").and_then(Value::as_str))
        .map(str::to_string);
    // 上下文：官方的 total_input_tokens 就是输入侧总量；没有时退回 current_usage 相加。
    // context_window_size 是上限的权威值（1M 变体也在这里体现）。
    let context = p.get("context_window").and_then(|cw| {
        let window = cw.get("context_window_size").and_then(Value::as_i64).unwrap_or(0);
        let used = cw
            .get("total_input_tokens")
            .and_then(Value::as_i64)
            .or_else(|| cw.get("current_usage").filter(|u| u.is_object()).map(usage_context_tokens))?;
        let window = if window > 0 { window } else { context_limit(model_id.unwrap_or(""), used) };
        (used > 0).then_some(ContextUsage { used_tokens: used, window_tokens: window })
    });
    let limits = p.get("rate_limits");
    Some(StatuslineData {
        session_id,
        tab_session_id: entry.tab_session_id.clone(),
        cwd,
        model,
        context,
        transcript_path: p.get("transcript_path").and_then(Value::as_str).map(str::to_string),
        primary: limits.and_then(|l| l.get("five_hour")).and_then(|w| statusline_window(w, 300)),
        secondary: limits.and_then(|l| l.get("seven_day")).and_then(|w| statusline_window(w, 10080)),
        received_at: entry.received_at,
    })
}

// ---------- hooks ----------

/// hooks 送来的最近事件 → 会话状态。返回 (状态, 送达时刻, 标签页会话 ID)。
fn hook_state(agent_dir: &Path, session_id: &str) -> Option<(SessionState, i64, Option<String>)> {
    let entry = read_entry(&agent_dir.join(claude_hook_file_name(session_id)))?;
    let state = match entry.payload.get("hook_event_name").and_then(Value::as_str) {
        Some("Notification") | Some("PermissionRequest") => SessionState::Waiting,
        Some("Stop") | Some("SessionEnd") => SessionState::Done,
        Some("SessionStart") => SessionState::Unknown,
        Some("UserPromptSubmit") | Some("PreToolUse") | Some("PostToolUse") => SessionState::Running,
        _ => SessionState::Unknown,
    };
    // 「运行中 / 等你确认」太久没有新事件，多半是 Claude Code 被直接关掉了（没机会发 SessionEnd），
    // 不再显示这个过期的状态
    let stale = crate::now_epoch() - entry.received_at > STALE_STATE_SECS;
    let state = if stale && matches!(state, SessionState::Running | SessionState::Waiting) {
        SessionState::Unknown
    } else {
        state
    };
    Some((state, entry.received_at, entry.tab_session_id))
}

/// 只来自 hooks 的会话（没接 statusLine、转录也还没写出回复的新会话）。
fn hook_only_sessions(agent_dir: &Path, known: &HashMap<String, SessionUsage>) -> Vec<SessionUsage> {
    let mut out = Vec::new();
    for path in list_entries(agent_dir, "claude-hook-") {
        let Some(entry) = read_entry(&path) else { continue };
        let Some(id) = entry.payload.get("session_id").and_then(Value::as_str) else { continue };
        if known.contains_key(id) || entry.payload.get("hook_event_name").and_then(Value::as_str) == Some("SessionEnd")
        {
            continue;
        }
        let Some((state, at, tab)) = hook_state(agent_dir, id) else { continue };
        out.push(SessionUsage {
            kind: AgentKind::Claude,
            session_id: id.to_string(),
            tab_session_id: tab,
            cwd: entry.payload.get("cwd").and_then(Value::as_str).map(str::to_string),
            model: None,
            context: None,
            state,
            updated_at: at,
            source: SnapshotSource::StatusLine,
        });
    }
    out
}

// ---------- 汇总 ----------

/// 采集 Claude Code：返回（会话快照，账号用量）。`agent_dir` 为 None 时不读 spool（扩展点都关了）。
pub fn collect(
    config_dir: &Path,
    agent_dir: Option<&Path>,
    read_files: bool,
) -> (Vec<SessionUsage>, Option<AccountUsage>) {
    let mut sessions: HashMap<String, SessionUsage> = HashMap::new();
    let mut account: Option<AccountUsage> = None;

    // 1. statusLine（官方扩展点，最优先）
    if let Some(dir) = agent_dir {
        for path in list_entries(dir, "claude-status-") {
            let Some(entry) = read_entry(&path) else { continue };
            let Some(data) = parse_statusline(&entry) else { continue };
            let context = data.context.clone().or_else(|| {
                data.transcript_path
                    .as_ref()
                    .filter(|_| read_files)
                    .and_then(|tp| transcript_tail(Path::new(tp)).context)
            });
            let (state, updated_at, tab) = match hook_state(dir, &data.session_id) {
                Some((s, at, tab)) => (s, at.max(data.received_at), tab),
                None => (SessionState::Unknown, data.received_at, None),
            };
            sessions.insert(
                data.session_id.clone(),
                SessionUsage {
                    kind: AgentKind::Claude,
                    session_id: data.session_id.clone(),
                    tab_session_id: data.tab_session_id.clone().or(tab),
                    cwd: data.cwd.clone(),
                    model: data.model.clone(),
                    context,
                    state,
                    updated_at,
                    source: SnapshotSource::StatusLine,
                },
            );
            if (data.primary.is_some() || data.secondary.is_some())
                && account.as_ref().is_none_or(|a| data.received_at > a.updated_at)
            {
                account = Some(AccountUsage {
                    kind: AgentKind::Claude,
                    primary: data.primary,
                    secondary: data.secondary,
                    plan: None,
                    updated_at: data.received_at,
                    source: SnapshotSource::StatusLine,
                });
            }
        }
    }

    // 2. 转录文件：补齐没接 statusLine 的会话
    if read_files {
        let mut files = Vec::new();
        collect_jsonl(&config_dir.join("projects"), &mut files);
        files.sort_by_key(|p| std::cmp::Reverse(file_mtime_epoch(p).unwrap_or(0)));
        files.truncate(MAX_TRANSCRIPTS);
        for path in &files {
            let info = transcript_tail(path);
            let (Some(session_id), Some(context)) = (info.session_id.clone(), info.context.clone()) else { continue };
            let mtime = file_mtime_epoch(path).unwrap_or(0);
            let hook = agent_dir.and_then(|d| hook_state(d, &session_id));
            sessions.entry(session_id.clone()).or_insert_with(|| SessionUsage {
                kind: AgentKind::Claude,
                session_id,
                tab_session_id: hook.as_ref().and_then(|h| h.2.clone()),
                cwd: info.cwd.clone(),
                model: info.model.clone(),
                context: Some(context),
                state: hook.as_ref().map(|h| h.0).unwrap_or(SessionState::Unknown),
                updated_at: hook.map(|h| h.1.max(mtime)).unwrap_or(mtime),
                source: SnapshotSource::SessionFile,
            });
        }
    }

    if let Some(dir) = agent_dir {
        for s in hook_only_sessions(dir, &sessions) {
            sessions.insert(s.session_id.clone(), s);
        }
    }

    let mut list: Vec<SessionUsage> = sessions.into_values().collect();
    list.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    list.truncate(MAX_SESSIONS);
    (list, account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spool::{claude_status_file_name, write_entry};
    use serde_json::json;

    /// 官方文档示例的 statusLine 输入（字段齐全的版本）
    fn statusline_payload() -> Value {
        json!({
            "hook_event_name": "Status",
            "session_id": "abc-123",
            "transcript_path": "/nonexistent/t.jsonl",
            "cwd": "/work/demo",
            "model": { "id": "claude-opus-4-7", "display_name": "Opus" },
            "workspace": { "current_dir": "/work/demo", "project_dir": "/work/demo" },
            "version": "2.1.302",
            "cost": { "total_cost_usd": 0.01 },
            "context_window": {
                "total_input_tokens": 15500, "total_output_tokens": 1200, "context_window_size": 1000000,
                "used_percentage": 1.55, "remaining_percentage": 98.45,
                "current_usage": { "input_tokens": 8500, "output_tokens": 1200, "cache_creation_input_tokens": 5000, "cache_read_input_tokens": 2000 }
            },
            "rate_limits": {
                "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
                "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
            }
        })
    }

    #[test]
    fn parses_statusline_with_rate_limits() {
        let entry =
            SpoolEntry { received_at: 100, tab_session_id: Some("tab-1".into()), payload: statusline_payload() };
        let d = parse_statusline(&entry).unwrap();
        assert_eq!(d.session_id, "abc-123");
        assert_eq!(d.model.as_deref(), Some("Opus"));
        assert_eq!(d.cwd.as_deref(), Some("/work/demo"));
        let ctx = d.context.unwrap();
        assert_eq!((ctx.used_tokens, ctx.window_tokens), (15500, 1_000_000));
        let p = d.primary.unwrap();
        assert_eq!((p.used_percent, p.window_minutes, p.resets_at), (23.5, Some(300), Some(1738425600)));
        assert_eq!(d.secondary.unwrap().window_minutes, Some(10080));
    }

    #[test]
    fn statusline_without_rate_limits_or_usage_degrades() {
        // API key 用户 / 首次响应之前：没有 rate_limits，current_usage 为 null
        let entry = SpoolEntry {
            received_at: 1,
            tab_session_id: None,
            payload: json!({ "session_id": "s", "model": { "id": "claude-sonnet-4-6" }, "context_window": { "current_usage": null } }),
        };
        let d = parse_statusline(&entry).unwrap();
        assert!(d.context.is_none() && d.primary.is_none() && d.secondary.is_none());
        assert_eq!(d.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn collect_merges_statusline_hooks_and_transcripts() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent");
        let now = crate::now_epoch();
        write_entry(
            &agent,
            &claude_status_file_name("abc-123"),
            &SpoolEntry { received_at: 1000, tab_session_id: Some("tab-1".into()), payload: statusline_payload() },
        )
        .unwrap();
        write_entry(
            &agent,
            &claude_hook_file_name("abc-123"),
            &SpoolEntry {
                received_at: now,
                tab_session_id: Some("tab-1".into()),
                payload: json!({ "hook_event_name": "Notification", "session_id": "abc-123" }),
            },
        )
        .unwrap();
        // 另一个只有转录的会话
        let proj = dir.path().join("claude").join("projects").join("C--work-other");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("zzz.jsonl"),
            concat!(
                r#"{"type":"user","sessionId":"zzz","cwd":"C:\\work\\other","message":{"role":"user","content":"hi"}}"#,
                "\n",
                r#"{"type":"assistant","sessionId":"zzz","cwd":"C:\\work\\other","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10,"cache_read_input_tokens":40000,"cache_creation_input_tokens":0,"output_tokens":5}}}"#,
                "\n"
            ),
        )
        .unwrap();
        let (list, account) = collect(&dir.path().join("claude"), Some(&agent), true);
        assert_eq!(list.len(), 2);
        let main = list.iter().find(|s| s.session_id == "abc-123").unwrap();
        assert_eq!(main.state, SessionState::Waiting);
        assert_eq!(main.tab_session_id.as_deref(), Some("tab-1"));
        assert_eq!(main.updated_at, now);
        let other = list.iter().find(|s| s.session_id == "zzz").unwrap();
        assert_eq!(other.context.as_ref().unwrap().used_tokens, 40010);
        assert_eq!(other.context.as_ref().unwrap().window_tokens, 200_000);
        assert_eq!(other.source, SnapshotSource::SessionFile);
        let account = account.unwrap();
        assert_eq!(account.kind, AgentKind::Claude);
        assert_eq!(account.primary.unwrap().used_percent, 23.5);
        // 关掉扩展点：只剩转录
        let (list, account) = collect(&dir.path().join("claude"), None, true);
        assert_eq!(list.len(), 1);
        assert!(account.is_none());
    }
}

#[cfg(test)]
mod stale_tests {
    use super::*;
    use crate::spool::write_entry;
    use serde_json::json;

    #[test]
    fn old_waiting_state_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let entry = SpoolEntry {
            received_at: crate::now_epoch() - 2 * 3600,
            tab_session_id: None,
            payload: json!({ "hook_event_name": "Notification", "session_id": "s1" }),
        };
        write_entry(dir.path(), &claude_hook_file_name("s1"), &entry).unwrap();
        assert_eq!(hook_state(dir.path(), "s1").unwrap().0, SessionState::Unknown);
    }
}
