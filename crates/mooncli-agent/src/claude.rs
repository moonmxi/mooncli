//! Claude Code 的数据（docs/07 第 2 节）：
//! 1. statusLine 输入（官方扩展点）：mooncli-bridge 作为 statusLine 命令，把 Claude Code 送来的
//!    JSON 写进 spool。里面有账号额度（rate_limits 的 five_hour / seven_day，claude.ai Pro/Max 订阅、
//!    首次 API 响应之后才有）和当前上下文（context_window）。额度只来自这里，不估算。
//! 2. hooks（官方扩展点）：bridge 记下每个会话最近一个事件（带转录路径和 mooncli 标签页），
//!    并在一轮结束、需要确认时发提醒（见 notify）。
//! 3. 会话转录 `~/.claude/projects/<项目>/<会话>.jsonl`（Claude Code 自己写的文件）：
//!    没接入 statusLine 时，用最后一条回复的 usage 补上下文占用。
//!
//! 字段以官方文档（code.claude.com/docs/en/statusline、hooks）为准，2026-10 核实；
//! 每个字段都「不存在就降级」，新旧版本字段有出入时不至于整体失效。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Focus;
use crate::jsonl::tail_lines;
use crate::model::{AccountUsage, AgentKind, ContextUsage, CurrentSession, RateWindow, SnapshotSource};
use crate::spool::{SpoolEntry, list_entries, read_entry};
use crate::timeutil::file_mtime_epoch;

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

/// 转录目录名：Claude Code 把项目路径里字母数字以外的字符都换成 `-`（`D:\work\app` → `D--work-app`）。
pub fn project_dir_name(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

// ---------- 转录 ----------

#[derive(Debug, Default)]
pub struct TranscriptInfo {
    pub model: Option<String>,
    pub context: Option<ContextUsage>,
    /// 最后一条回复的文字（提醒里显示一句摘要）
    pub last_text: Option<String>,
}

fn message_text(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// 尾部扫描一个转录文件：最后一条真实回复（不是 `<synthetic>`）的 usage → 上下文占用，
/// 最后一条带文字的回复 → 摘要。
pub fn transcript_tail(path: &Path) -> TranscriptInfo {
    let mut info = TranscriptInfo::default();
    let mut tail = 256 * 1024u64;
    loop {
        for line in tail_lines(path, tail).iter().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            if v.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(msg) = v.get("message") else { continue };
            let model = msg.get("model").and_then(Value::as_str).unwrap_or("");
            if model.is_empty() || model == "<synthetic>" {
                continue;
            }
            if info.context.is_none()
                && let Some(usage) = msg.get("usage")
            {
                let used = usage_context_tokens(usage);
                if used > 0 {
                    info.model = Some(model.to_string());
                    info.context = Some(ContextUsage::simple(used, context_limit(model, used)));
                }
            }
            if info.last_text.is_none() {
                info.last_text = message_text(msg);
            }
            if info.context.is_some() && info.last_text.is_some() {
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
    pub model: Option<String>,
    pub context: Option<ContextUsage>,
    pub transcript_path: Option<String>,
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
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
    })
}

pub fn parse_statusline(p: &Value) -> Option<StatuslineData> {
    let session_id = p.get("session_id")?.as_str()?.to_string();
    let model_obj = p.get("model");
    let model_id = model_obj.and_then(|m| m.get("id")).and_then(Value::as_str);
    let model = model_obj.and_then(|m| m.get("display_name")).and_then(Value::as_str).or(model_id).map(str::to_string);
    // 上下文：current_usage 是最近一次请求的输入侧（total_input_tokens 是整个会话累计，不能用）；
    // context_window_size 是上限的权威值，used_percentage 是 Claude Code 自己算的百分比。
    let context = p.get("context_window").and_then(|cw| {
        let used = cw.get("current_usage").filter(|u| u.is_object()).map(usage_context_tokens)?;
        if used <= 0 {
            return None;
        }
        let window = cw
            .get("context_window_size")
            .and_then(Value::as_i64)
            .filter(|w| *w > 0)
            .unwrap_or_else(|| context_limit(model_id.unwrap_or(""), used));
        let mut ctx = ContextUsage::simple(used, window);
        if let Some(pct) = cw.get("used_percentage").and_then(Value::as_f64) {
            ctx.used_percent = pct.clamp(0.0, 100.0);
        }
        Some(ctx)
    });
    let limits = p.get("rate_limits");
    Some(StatuslineData {
        session_id,
        model,
        context,
        transcript_path: p.get("transcript_path").and_then(Value::as_str).map(str::to_string),
        primary: limits.and_then(|l| l.get("five_hour")).and_then(|w| statusline_window(w, 300)),
        secondary: limits.and_then(|l| l.get("seven_day")).and_then(|w| statusline_window(w, 10080)),
    })
}

/// 账号额度：所有会话里最新一份带 rate_limits 的 statusLine（额度是账号级的，哪个会话报的都一样）。
pub fn latest_account(agent_dir: &Path) -> Option<AccountUsage> {
    let mut best: Option<AccountUsage> = None;
    for path in list_entries(agent_dir, "claude-status-") {
        let Some(entry) = read_entry(&path) else { continue };
        if best.as_ref().is_some_and(|b| b.updated_at >= entry.received_at) {
            continue;
        }
        let Some(d) = parse_statusline(&entry.payload) else { continue };
        if let Some(a) = AccountUsage::from_windows(
            AgentKind::Claude,
            d.primary,
            d.secondary,
            None,
            entry.received_at,
            SnapshotSource::StatusLine,
        ) {
            best = Some(a);
        }
    }
    best
}

// ---------- 当前会话 ----------

/// 这个标签页、本次运行以来最新的一条 spool 记录
fn newest_for_tab(agent_dir: &Path, prefix: &str, focus: &Focus) -> Option<SpoolEntry> {
    list_entries(agent_dir, prefix)
        .iter()
        .filter_map(|p| read_entry(p))
        .filter(|e| e.tab_session_id.as_deref() == Some(focus.tab_id.as_str()) && e.received_at >= focus.since)
        .max_by_key(|e| e.received_at)
}

/// 当前标签页里正在跑的 Claude Code 会话的上下文。依次看：
/// 1. 这个标签页送来的 statusLine（最准，接入后每次回复都会更新）；
/// 2. 这个标签页的 hooks 记下的转录路径；
/// 3. 都没接入时，按标签页目录找这次运行以来写过的转录。
pub fn current(claude_dir: &Path, agent_dir: &Path, focus: &Focus) -> CurrentSession {
    let mut cur = CurrentSession { kind: AgentKind::Claude, model: None, context: None, updated_at: focus.since };
    if let Some(entry) = newest_for_tab(agent_dir, "claude-status-", focus)
        && let Some(d) = parse_statusline(&entry.payload)
    {
        cur.model = d.model;
        cur.updated_at = entry.received_at;
        cur.context = d.context.or_else(|| d.transcript_path.and_then(|t| transcript_tail(Path::new(&t)).context));
        if cur.context.is_some() {
            return cur;
        }
    }
    let transcript = newest_for_tab(agent_dir, "claude-hook-", focus)
        .and_then(|e| e.payload.get("transcript_path").and_then(Value::as_str).map(PathBuf::from))
        .filter(|p| p.is_file())
        .or_else(|| {
            let cwd = focus.cwd.as_deref()?;
            newest_transcript(&claude_dir.join("projects").join(project_dir_name(cwd)), focus.since)
        });
    if let Some(path) = transcript {
        // 转录文件名就是会话 ID：这个会话送过 statusLine 的话，用它的上下文（上限是权威值，转录里没有）
        let session = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if let Some(entry) = read_entry(&agent_dir.join(crate::spool::claude_status_file_name(session)))
            && entry.received_at >= focus.since
            && let Some(d) = parse_statusline(&entry.payload).filter(|d| d.context.is_some())
        {
            cur.model = d.model;
            cur.context = d.context;
            cur.updated_at = entry.received_at;
            return cur;
        }
        let info = transcript_tail(&path);
        if info.context.is_some() {
            cur.updated_at = cur.updated_at.max(file_mtime_epoch(&path).unwrap_or(0));
            cur.model = cur.model.or(info.model);
            cur.context = info.context;
        }
    }
    cur
}

/// 目录里 since 之后改过的最新转录
fn newest_transcript(dir: &Path, since: i64) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| file_mtime_epoch(&p).map(|t| (t, p)))
        .filter(|(t, _)| *t >= since)
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spool::{claude_hook_file_name, claude_status_file_name, write_entry};
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
            "context_window": {
                "total_input_tokens": 915500, "total_output_tokens": 1200, "context_window_size": 1000000,
                "used_percentage": 1.6, "remaining_percentage": 98.4,
                "current_usage": { "input_tokens": 8500, "output_tokens": 1200, "cache_creation_input_tokens": 5000, "cache_read_input_tokens": 2000 }
            },
            "rate_limits": {
                "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
                "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
            }
        })
    }

    fn focus(since: i64, cwd: Option<&str>) -> Focus {
        Focus { kind: AgentKind::Claude, tab_id: "tab-1".into(), cwd: cwd.map(str::to_string), since }
    }

    #[test]
    fn parses_statusline_context_from_current_usage() {
        let d = parse_statusline(&statusline_payload()).unwrap();
        assert_eq!(d.session_id, "abc-123");
        assert_eq!(d.model.as_deref(), Some("Opus"));
        let ctx = d.context.unwrap();
        // 用最近一次请求的输入侧，不用整个会话累计的 total_input_tokens
        assert_eq!((ctx.used_tokens, ctx.window_tokens, ctx.used_percent), (15500, 1_000_000, 1.6));
        let p = d.primary.unwrap();
        assert_eq!((p.used_percent, p.window_minutes, p.resets_at), (23.5, Some(300), Some(1738425600)));
        assert_eq!(d.secondary.unwrap().window_minutes, Some(10080));
    }

    #[test]
    fn statusline_without_rate_limits_or_usage_degrades() {
        // API key 用户 / 首次响应之前：没有 rate_limits，current_usage 为 null
        let p = json!({ "session_id": "s", "model": { "id": "claude-sonnet-4-6" }, "context_window": { "current_usage": null } });
        let d = parse_statusline(&p).unwrap();
        assert!(d.context.is_none() && d.primary.is_none() && d.secondary.is_none());
        assert_eq!(d.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn account_comes_from_newest_statusline() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = statusline_payload();
        old["session_id"] = json!("old");
        old["rate_limits"]["five_hour"]["used_percentage"] = json!(5.0);
        write_entry(
            dir.path(),
            &claude_status_file_name("old"),
            &SpoolEntry { received_at: 10, tab_session_id: None, payload: old },
        )
        .unwrap();
        write_entry(
            dir.path(),
            &claude_status_file_name("abc-123"),
            &SpoolEntry { received_at: 20, tab_session_id: None, payload: statusline_payload() },
        )
        .unwrap();
        let a = latest_account(dir.path()).unwrap();
        assert_eq!(a.updated_at, 20);
        assert_eq!(a.five_hour.unwrap().used_percent, 23.5);
        assert_eq!(a.seven_day.unwrap().used_percent, 41.2);
    }

    #[test]
    fn current_prefers_this_tabs_statusline_since_start() {
        let dir = tempfile::tempdir().unwrap();
        let entry = |at: i64, tab: &str| SpoolEntry {
            received_at: at,
            tab_session_id: Some(tab.into()),
            payload: statusline_payload(),
        };
        write_entry(dir.path(), &claude_status_file_name("abc-123"), &entry(100, "tab-1")).unwrap();
        let cur = current(&dir.path().join("claude"), dir.path(), &focus(50, None));
        assert_eq!(cur.context.unwrap().used_tokens, 15500);
        assert_eq!(cur.model.as_deref(), Some("Opus"));
        // 这次运行之前的记录、别的标签页的记录都不算
        assert!(current(&dir.path().join("claude"), dir.path(), &focus(200, None)).context.is_none());
        write_entry(dir.path(), &claude_status_file_name("abc-123"), &entry(100, "tab-2")).unwrap();
        assert!(current(&dir.path().join("claude"), dir.path(), &focus(50, None)).context.is_none());
    }

    const TRANSCRIPT: &str = concat!(
        r#"{"type":"user","sessionId":"zzz","message":{"role":"user","content":"hi"}}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"zzz","message":{"model":"claude-sonnet-4-6","content":[{"type":"text","text":"改好了，测试都过了。"}],"usage":{"input_tokens":10,"cache_read_input_tokens":40000,"cache_creation_input_tokens":0,"output_tokens":5}}}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"zzz","message":{"model":"<synthetic>","content":[{"type":"text","text":"x"}],"usage":{"input_tokens":0}}}"#,
        "\n"
    );

    #[test]
    fn transcript_tail_reads_context_and_last_text() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, TRANSCRIPT).unwrap();
        let info = transcript_tail(&p);
        let ctx = info.context.unwrap();
        assert_eq!((ctx.used_tokens, ctx.window_tokens), (40010, 200_000));
        assert_eq!(info.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(info.last_text.as_deref(), Some("改好了，测试都过了。"));
    }

    #[test]
    fn current_falls_back_to_hook_transcript_then_project_dir() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent");
        let claude = dir.path().join("claude");
        // 只有按目录找得到的转录
        let proj = claude.join("projects").join(project_dir_name(r"C:\work\demo"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("zzz.jsonl"), TRANSCRIPT).unwrap();
        let cur = current(&claude, &agent, &focus(0, Some(r"C:\work\demo")));
        assert_eq!(cur.context.unwrap().used_tokens, 40010);
        // 目录对不上就没有
        assert!(current(&claude, &agent, &focus(0, Some(r"C:\work\other"))).context.is_none());
        // hooks 记下的转录路径优先，不依赖目录
        let t = dir.path().join("elsewhere.jsonl");
        std::fs::write(&t, TRANSCRIPT).unwrap();
        write_entry(
            &agent,
            &claude_hook_file_name("zzz"),
            &SpoolEntry {
                received_at: 10,
                tab_session_id: Some("tab-1".into()),
                payload: json!({ "session_id": "zzz", "hook_event_name": "Stop", "transcript_path": t.display().to_string() }),
            },
        )
        .unwrap();
        assert_eq!(current(&claude, &agent, &focus(0, None)).context.unwrap().used_tokens, 40010);
    }

    #[test]
    fn project_dir_names_match_claude_code() {
        assert_eq!(project_dir_name(r"D:\projects\mooncli-dev"), "D--projects-mooncli-dev");
        assert_eq!(project_dir_name("/home/me/my_app"), "-home-me-my-app");
    }
}
