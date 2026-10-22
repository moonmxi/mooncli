//! Claude Code 的数据：
//! 1. statusLine / hooks 经 mooncli-bridge 写进 spool 的条目（官方扩展点，最新最准）；
//! 2. 会话转录 `~/.claude/projects/<项目>/<会话>.jsonl`（Claude Code 自己写的文件）：
//!    最后一条回复的 usage 给上下文占用；全部回复的 usage 按 ccusage 的 5 小时块算法汇总，
//!    在拿不到官方额度字段时给估算（只报 token 数和重置时间，不编百分比，见 docs/07 第 2 节）。
//!
//! statusLine 输入的字段以官方文档为准逐个核实（docs/07 附录）；这里对每个字段都做
//! 「不存在就降级」的处理，新旧版本字段有出入时不至于整体失效。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonl::{collect_jsonl, tail_lines};
use crate::model::{AccountUsage, AgentKind, ContextUsage, RateWindow, SessionState, SessionUsage, SnapshotSource};
use crate::spool::{hook_file_name, list_entries, read_entry};
use crate::timeutil::{file_mtime_epoch, now_epoch, parse_rfc3339_epoch};

const MAX_TRANSCRIPTS: usize = 40;
const BLOCK_SECONDS: i64 = 5 * 3600;

/// Claude Code 配置目录：CLAUDE_CONFIG_DIR 优先，否则 ~/.claude。
pub fn claude_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    std::env::home_dir().map(|h| h.join(".claude")).unwrap_or_default()
}

/// 模型上下文上限：模型 ID 带 `[1m]` 后缀的是 100 万变体，其余按 20 万。
fn context_limit(model: &str) -> i64 {
    if model.contains("[1m]") {
        1_000_000
    } else {
        200_000
    }
}

fn usage_context_tokens(usage: &Value) -> i64 {
    let get = |k: &str| usage.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    get("input_tokens") + get("cache_read_input_tokens") + get("cache_creation_input_tokens")
}

fn usage_total_tokens(usage: &Value) -> i64 {
    usage_context_tokens(usage) + usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
}

// ---------- 转录 ----------

pub struct TranscriptInfo {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub context: Option<ContextUsage>,
}

/// 尾部扫描一个转录文件：最后一条真实回复的 usage → 上下文占用。
pub fn transcript_tail(path: &Path) -> TranscriptInfo {
    let mut info = TranscriptInfo { session_id: None, cwd: None, model: None, context: None };
    let mut tail = 256 * 1024u64;
    loop {
        let lines = tail_lines(path, tail);
        for line in lines.iter().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            if info.session_id.is_none() {
                info.session_id = v.get("sessionId").and_then(|s| s.as_str()).map(|s| s.to_string());
            }
            if info.cwd.is_none() {
                info.cwd = v.get("cwd").and_then(|s| s.as_str()).map(|s| s.to_string());
            }
            if info.context.is_none() && v.get("type").and_then(|t| t.as_str()) == Some("assistant") {
                if let Some(msg) = v.get("message") {
                    let model = msg.get("model").and_then(|m| m.as_str()).unwrap_or("");
                    if !model.is_empty() && model != "<synthetic>" {
                        if let Some(usage) = msg.get("usage") {
                            let used = usage_context_tokens(usage);
                            if used > 0 {
                                info.model = Some(model.to_string());
                                info.context = Some(ContextUsage { used_tokens: used, window_tokens: context_limit(model) });
                            }
                        }
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

/// 一个转录文件里全部回复的用量事件：(时刻 epoch 秒, 总 token 数)。
/// 同一条消息在流式写入时会出现多次，按 message.id + requestId 去重（ccusage 的做法）。
pub fn transcript_events(path: &Path) -> Vec<(i64, i64)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut events = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(msg) = v.get("message") else { continue };
        let model = msg.get("model").and_then(|m| m.as_str()).unwrap_or("");
        if model.is_empty() || model == "<synthetic>" {
            continue;
        }
        let (Some(usage), Some(ts)) = (msg.get("usage"), v.get("timestamp").and_then(|t| t.as_str())) else {
            continue;
        };
        let Some(epoch) = parse_rfc3339_epoch(ts) else { continue };
        let dedupe_key = format!(
            "{}|{}",
            msg.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            v.get("requestId").and_then(|i| i.as_str()).unwrap_or("")
        );
        if dedupe_key != "|" && !seen.insert(dedupe_key) {
            continue;
        }
        let total = usage_total_tokens(usage);
        if total > 0 {
            events.push((epoch, total));
        }
    }
    events
}

/// 5 小时块估算：服务端的窗口由首条消息开启、持续 5 小时（帮助中心口径），
/// 所以块起点就是块内第一个事件的时刻，不取整到整点；5 小时内的事件都算这一块，
/// 超出后从下一个事件重新开块。返回当前（未过期）块的 (开始时刻, 累计 token)。
pub fn current_block(events: &[(i64, i64)], now: i64) -> Option<(i64, i64)> {
    if events.is_empty() {
        return None;
    }
    let mut sorted: Vec<(i64, i64)> = events.to_vec();
    sorted.sort_by_key(|(t, _)| *t);
    let mut block_start: Option<i64> = None;
    let mut block_total: i64 = 0;
    for (t, total) in sorted {
        match block_start {
            Some(start) if t < start + BLOCK_SECONDS => {
                block_total += total;
            }
            _ => {
                block_start = Some(t);
                block_total = total;
            }
        }
    }
    let start = block_start?;
    if now < start + BLOCK_SECONDS {
        Some((start, block_total))
    } else {
        None
    }
}

// ---------- statusLine ----------

struct StatuslineData {
    session_id: String,
    tab_session_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    context: Option<ContextUsage>,
    cost_usd: Option<f64>,
    transcript_path: Option<String>,
    primary: Option<RateWindow>,
    secondary: Option<RateWindow>,
    received_at: i64,
}

/// resets_at 在 statusLine 里可能是 epoch 秒数字，也可能是 RFC3339 字符串，两种都认。
fn resets_at_value(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    if let Some(f) = v.as_f64() {
        return Some(f as i64);
    }
    v.as_str().and_then(parse_rfc3339_epoch)
}

fn statusline_window(v: &Value, window_minutes: i64) -> Option<RateWindow> {
    let used = v.get("used_percentage")?.as_f64()?;
    Some(RateWindow {
        used_percent: used,
        window_minutes: Some(window_minutes),
        resets_at: v.get("resets_at").and_then(resets_at_value),
        estimated: false,
    })
}

fn parse_statusline(entry: &crate::spool::SpoolEntry) -> Option<StatuslineData> {
    let p = &entry.payload;
    let session_id = p.get("session_id")?.as_str()?.to_string();
    let model_obj = p.get("model");
    let model_id = model_obj.and_then(|m| m.get("id")).and_then(|m| m.as_str());
    let model = model_obj
        .and_then(|m| m.get("display_name"))
        .and_then(|m| m.as_str())
        .or(model_id)
        .map(|s| s.to_string());
    let cwd = p
        .get("workspace")
        .and_then(|w| w.get("current_dir"))
        .and_then(|c| c.as_str())
        .or_else(|| p.get("cwd").and_then(|c| c.as_str()))
        .map(|s| s.to_string());
    // 上下文：官方口径优先——context_window.total_input_tokens 就是输入侧总量
    // （input + cache_creation + cache_read，不含 output）；没有时退回 current_usage 相加，
    // 再没有就靠转录补（见 collect）。context_window_size 是上限的权威值。
    let context = p.get("context_window").and_then(|cw| {
        let window = cw
            .get("context_window_size")
            .and_then(|w| w.as_i64())
            .unwrap_or_else(|| context_limit(model_id.unwrap_or("")));
        let used = cw
            .get("total_input_tokens")
            .and_then(|t| t.as_i64())
            .or_else(|| cw.get("current_usage").map(usage_context_tokens))?;
        if used > 0 {
            Some(ContextUsage { used_tokens: used, window_tokens: window })
        } else {
            None
        }
    });
    let limits = p.get("rate_limits");
    Some(StatuslineData {
        session_id,
        tab_session_id: entry.tab_session_id.clone(),
        cwd,
        model,
        context,
        cost_usd: p.get("cost").and_then(|c| c.get("total_cost_usd")).and_then(|c| c.as_f64()),
        transcript_path: p.get("transcript_path").and_then(|t| t.as_str()).map(|s| s.to_string()),
        primary: limits.and_then(|l| l.get("five_hour")).and_then(|w| statusline_window(w, 300)),
        secondary: limits.and_then(|l| l.get("seven_day")).and_then(|w| statusline_window(w, 10080)),
        received_at: entry.received_at,
    })
}

/// hooks 送来的最近状态：notification → 等待确认，stop → 一轮结束。
fn hook_state(agent_dir: &Path, session_id: &str, enabled: bool) -> SessionState {
    if !enabled {
        return SessionState::Unknown;
    }
    let path = agent_dir.join(hook_file_name(session_id));
    let Some(entry) = read_entry(&path) else {
        return SessionState::Unknown;
    };
    match entry.payload.get("hook_event_name").and_then(|e| e.as_str()) {
        Some("Notification") => SessionState::Waiting,
        Some("Stop") => SessionState::Done,
        Some("SessionStart") | Some("UserPromptSubmit") | Some("PreToolUse") | Some("PostToolUse") => SessionState::Running,
        _ => SessionState::Unknown,
    }
}

// ---------- 汇总 ----------

/// 采集 Claude Code：返回（会话快照，账号用量）。
pub fn collect(config_dir: &Path, agent_dir: &Path, sources: crate::Sources) -> (Vec<SessionUsage>, Option<AccountUsage>) {
    let mut sessions: HashMap<String, SessionUsage> = HashMap::new();
    let mut account: Option<AccountUsage> = None;

    // 1. statusLine spool（官方扩展点，最优先）
    if sources.statusline {
        for path in list_entries(agent_dir, "claude-status-") {
            let Some(entry) = read_entry(&path) else { continue };
            let Some(data) = parse_statusline(&entry) else { continue };
            // 上下文缺失时按它给的 transcript_path 补
            let context = data.context.clone().or_else(|| {
                data.transcript_path.as_ref().and_then(|tp| transcript_tail(Path::new(tp)).context)
            });
            let state = hook_state(agent_dir, &data.session_id, sources.hooks);
            sessions.insert(
                data.session_id.clone(),
                SessionUsage {
                    kind: AgentKind::Claude,
                    session_id: data.session_id.clone(),
                    tab_session_id: data.tab_session_id.clone(),
                    cwd: data.cwd.clone(),
                    model: data.model.clone(),
                    context,
                    cost_usd: data.cost_usd,
                    state: if state == SessionState::Unknown { SessionState::Running } else { state },
                    updated_at: data.received_at,
                    source: SnapshotSource::StatusLine,
                },
            );
            if (data.primary.is_some() || data.secondary.is_some())
                && account.as_ref().is_none_or(|a: &AccountUsage| data.received_at > a.updated_at)
            {
                account = Some(AccountUsage {
                    kind: AgentKind::Claude,
                    primary: data.primary.clone(),
                    secondary: data.secondary.clone(),
                    plan: None,
                    window_tokens: None,
                    updated_at: data.received_at,
                    source: SnapshotSource::StatusLine,
                });
            }
        }
    }

    // 2. 转录文件：补齐没接 statusLine 的会话，并为估算收集事件
    if sources.files {
        let projects = config_dir.join("projects");
        let mut files = Vec::new();
        collect_jsonl(&projects, &mut files);
        files.sort_by_key(|p| std::cmp::Reverse(file_mtime_epoch(p).unwrap_or(0)));
        let recent_for_events: Vec<PathBuf> = files
            .iter()
            .filter(|p| now_epoch() - file_mtime_epoch(p).unwrap_or(0) < 24 * 3600)
            .cloned()
            .collect();
        files.truncate(MAX_TRANSCRIPTS);
        for path in &files {
            let info = transcript_tail(path);
            let Some(session_id) = info.session_id.clone() else { continue };
            let updated_at = file_mtime_epoch(path).unwrap_or(0);
            sessions.entry(session_id.clone()).or_insert_with(|| SessionUsage {
                kind: AgentKind::Claude,
                session_id: session_id.clone(),
                tab_session_id: None,
                cwd: info.cwd.clone(),
                model: info.model.clone(),
                context: info.context.clone(),
                cost_usd: None,
                state: hook_state(agent_dir, &session_id, sources.hooks),
                updated_at,
                source: SnapshotSource::SessionFile,
            });
        }

        // 3. 没有官方额度字段时，给 5 小时块估算（只报 token 数和重置时间）
        if account.is_none() {
            let mut events = Vec::new();
            for path in &recent_for_events {
                events.extend(transcript_events(path));
            }
            if let Some((start, total)) = current_block(&events, now_epoch()) {
                account = Some(AccountUsage {
                    kind: AgentKind::Claude,
                    primary: Some(RateWindow {
                        used_percent: 0.0,
                        window_minutes: Some(300),
                        resets_at: Some(start + BLOCK_SECONDS),
                        estimated: true,
                    }),
                    secondary: None,
                    plan: None,
                    window_tokens: Some(total),
                    updated_at: now_epoch(),
                    source: SnapshotSource::Estimate,
                });
            }
        }
    }

    let mut list: Vec<SessionUsage> = sessions.into_values().collect();
    list.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    (list, account)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_algorithm_groups_and_expires() {
        let base = 1792627200; // 2026-10-22T00:00:00Z
        let events = vec![(base + 60, 100), (base + 3600, 50), (base + 5 * 3600 + 60, 70)];
        // 块从第一个事件的时刻开始（不取整）：第一块 [base+60, base+60+5h)
        // 第三个事件正好落在块尾之外，开新块；now 在第二块内时只剩 70
        assert_eq!(current_block(&events, base + 5 * 3600 + 120), Some((base + 5 * 3600 + 60, 70)));
        // 只给第一块的事件时：
        assert_eq!(current_block(&events[..2], base + 7200), Some((base + 60, 150)));
        // 块过期后无当前块
        assert_eq!(current_block(&events[..2], base + 60 + 5 * 3600 + 1), None);
    }

    #[test]
    fn parses_statusline_with_rate_limits() {
        let payload = serde_json::json!({
            "session_id": "abc-123",
            "transcript_path": "/tmp/t.jsonl",
            "cwd": "/work/demo",
            "model": {"id": "claude-opus-4-1", "display_name": "Opus 4.1"},
            "workspace": {"current_dir": "/work/demo", "project_dir": "/work/demo"},
            "cost": {"total_cost_usd": 0.42},
            "context_window": {
                "total_input_tokens": 126000,
                "total_output_tokens": 5000,
                "context_window_size": 200000,
                "used_percentage": 63.0,
                "current_usage": {"input_tokens": 2000, "output_tokens": 500, "cache_creation_input_tokens": 4000, "cache_read_input_tokens": 120000}
            },
            "rate_limits": {
                "five_hour": {"used_percentage": 42.0, "resets_at": 1792700000},
                "seven_day": {"used_percentage": 18.0, "resets_at": "2026-10-29T00:00:00Z"}
            }
        });
        let entry = crate::spool::SpoolEntry { received_at: 100, tab_session_id: Some("tab-1".into()), payload };
        let data = parse_statusline(&entry).expect("应能解析");
        assert_eq!(data.session_id, "abc-123");
        assert_eq!(data.model.as_deref(), Some("Opus 4.1"));
        assert_eq!(data.cost_usd, Some(0.42));
        // 上下文用官方 total_input_tokens（126000），不是 current_usage 相加（也是 126000，此处验证取官方总量字段优先且上限取 context_window_size）
        let ctx = data.context.expect("应有上下文");
        assert_eq!(ctx.used_tokens, 126000);
        assert_eq!(ctx.window_tokens, 200000);
        let primary = data.primary.unwrap();
        assert_eq!(primary.used_percent, 42.0);
        assert_eq!(primary.resets_at, Some(1792700000));
        let secondary = data.secondary.unwrap();
        assert_eq!(secondary.resets_at, Some(1792627200 + 7 * 86400));
    }

    #[test]
    fn transcript_tail_reads_last_usage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let lines = [
            r#"{"type":"user","sessionId":"s1","cwd":"/w","timestamp":"2026-10-22T02:00:00.000Z","message":{"role":"user","content":"hi"}}"#,
            r#"{"type":"assistant","sessionId":"s1","cwd":"/w","timestamp":"2026-10-22T02:00:05.000Z","requestId":"r1","message":{"id":"m1","model":"claude-sonnet-4-5","usage":{"input_tokens":100,"cache_read_input_tokens":50,"cache_creation_input_tokens":10,"output_tokens":20}}}"#,
            r#"{"type":"assistant","sessionId":"s1","cwd":"/w","timestamp":"2026-10-22T02:01:05.000Z","requestId":"r2","message":{"id":"m2","model":"claude-sonnet-4-5[1m]","usage":{"input_tokens":200,"cache_read_input_tokens":900,"cache_creation_input_tokens":0,"output_tokens":30}}}"#,
        ];
        std::fs::write(&path, lines.join("\n")).unwrap();
        let info = transcript_tail(&path);
        let ctx = info.context.expect("应有上下文");
        assert_eq!(ctx.used_tokens, 1100);
        assert_eq!(ctx.window_tokens, 1_000_000);
        // 事件去重：两条不同 requestId 都算
        let events = transcript_events(&path);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].1, 180);
    }
}
