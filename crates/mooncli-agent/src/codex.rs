//! Codex 会话文件（rollout JSONL）解析。格式依据 openai/codex 源码核实（2026-10-22）：
//! - 文件在 `$CODEX_HOME/sessions/年/月/日/rollout-*.jsonl`，归档的在 `archived_sessions/`。
//! - 每行是 `{timestamp, type, payload}`；首行 `type = "session_meta"`，payload 带 `id`、`cwd`。
//! - 用量事件是 `type = "event_msg"` 且 `payload.type = "token_count"`：payload 的 `info`
//!   给 token 用量和 `model_context_window`，`rate_limits` 是后端下发、客户端落盘的额度快照
//!   （primary/secondary 各带 used_percent、window_minutes、resets_at epoch 秒、plan_type）。
//!   读它等于读官方 /status 的同源数据，不需要任何凭证。
//! - 窗口语义认 window_minutes 动态判断，任一窗口都可能缺席（例如 Pro 没有 5 小时窗口）。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonl::{collect_jsonl, first_line, tail_lines};
use crate::model::{AccountUsage, AgentKind, ContextUsage, RateWindow, SessionState, SessionUsage};
use crate::spool::{list_entries, read_entry};
use crate::timeutil::file_mtime_epoch;

/// 一次最多看多少个最近的会话文件，避免老机器上扫全历史。
const MAX_FILES: usize = 60;
/// 小窗最多列多少个 Codex 会话。
const MAX_SESSIONS: usize = 20;
/// 尾部扫描的起始/最大范围（字节）。token_count 每轮都有，通常一探即中。
const TAIL_START: u64 = 256 * 1024;
const TAIL_MAX: u64 = 4 * 1024 * 1024;

/// Codex 数据目录：环境变量 CODEX_HOME（可逗号分隔多个）优先，否则 ~/.codex。
pub fn codex_homes() -> Vec<PathBuf> {
    if let Ok(raw) = std::env::var("CODEX_HOME") {
        let homes: Vec<PathBuf> =
            raw.split(',').map(|s| PathBuf::from(s.trim())).filter(|p| !p.as_os_str().is_empty()).collect();
        if !homes.is_empty() {
            return homes;
        }
    }
    std::env::home_dir().map(|h| vec![h.join(".codex")]).unwrap_or_default()
}

pub struct TokenCount {
    pub context: Option<ContextUsage>,
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
    pub plan: Option<String>,
}

fn rate_window(v: &Value) -> Option<RateWindow> {
    let used = v.get("used_percent")?.as_f64()?;
    Some(RateWindow {
        used_percent: used,
        window_minutes: v.get("window_minutes").and_then(|x| x.as_i64()),
        resets_at: v.get("resets_at").and_then(|x| x.as_i64()),
        estimated: false,
    })
}

fn parse_token_count(payload: &Value) -> TokenCount {
    let info = payload.get("info");
    let context = info.and_then(|info| {
        let window = info.get("model_context_window")?.as_i64()?;
        let used = info.get("last_token_usage")?.get("input_tokens")?.as_i64()?;
        Some(ContextUsage { used_tokens: used, window_tokens: window })
    });
    let limits = payload.get("rate_limits");
    TokenCount {
        context,
        primary: limits.and_then(|l| l.get("primary")).and_then(rate_window),
        secondary: limits.and_then(|l| l.get("secondary")).and_then(rate_window),
        plan: limits.and_then(|l| l.get("plan_type")).and_then(|p| p.as_str()).map(|s| s.to_string()),
    }
}

pub struct RolloutInfo {
    pub session_id: String,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub token_count: Option<TokenCount>,
}

/// 解析一个 rollout 文件：首行 session_meta + 尾部最新 token_count / turn_context。
pub fn parse_rollout(path: &Path) -> Option<RolloutInfo> {
    let meta_line = first_line(path)?;
    let meta: Value = serde_json::from_str(&meta_line).ok()?;
    if meta.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let payload = meta.get("payload")?;
    let session_id = payload.get("id")?.as_str()?.to_string();
    let mut cwd = payload.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());

    // 尾部扫描：从后往前找最后一个 token_count 和最后一个 turn_context。
    let mut token_count = None;
    let mut model = None;
    let mut tail = TAIL_START;
    loop {
        let lines = tail_lines(path, tail);
        for line in lines.iter().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("event_msg") => {
                    let p = v.get("payload");
                    if token_count.is_none()
                        && p.and_then(|p| p.get("type")).and_then(|t| t.as_str()) == Some("token_count")
                    {
                        token_count = Some(parse_token_count(p.unwrap()));
                    }
                }
                Some("turn_context") => {
                    if model.is_none()
                        && let Some(p) = v.get("payload")
                    {
                        model = p.get("model").and_then(|m| m.as_str()).map(|s| s.to_string());
                        if let Some(c) = p.get("cwd").and_then(|c| c.as_str()) {
                            cwd = Some(c.to_string());
                        }
                    }
                }
                _ => {}
            }
            if token_count.is_some() && model.is_some() {
                break;
            }
        }
        if token_count.is_some() || tail >= TAIL_MAX {
            break;
        }
        tail *= 4;
    }

    Some(RolloutInfo { session_id, cwd, model, token_count })
}

/// 把 notify 的记录并进会话快照（docs/07 第 4 节）：notify 目前只有 agent-turn-complete
/// 一个事件——它比会话文件新时，该会话标为「本轮已完成」，并带上 mooncli 标签页的关联。
fn apply_notify(sessions: &mut [SessionUsage], agent_dir: &Path) {
    for path in list_entries(agent_dir, "codex-notify-") {
        let Some(entry) = read_entry(&path) else { continue };
        let Some(thread_id) = entry.payload.get("thread-id").and_then(|v| v.as_str()) else { continue };
        let Some(session) = sessions.iter_mut().find(|s| s.session_id == thread_id) else { continue };
        if session.tab_session_id.is_none() {
            session.tab_session_id = entry.tab_session_id.clone();
        }
        if entry.received_at >= session.updated_at {
            if entry.payload.get("type").and_then(|v| v.as_str()) == Some("agent-turn-complete") {
                session.state = SessionState::Done;
            }
            session.updated_at = entry.received_at;
        }
    }
}

/// 扫描所有 Codex 数据目录，返回（会话快照，账号用量）。
/// 账号额度取 mtime 最新文件里的 rate_limits——那是该账号最近一次模型响应落盘的快照。
/// `agent_dir` 给了且里面有 notify 的 spool 记录时，并入会话状态（见 apply_notify）。
pub fn collect(homes: &[PathBuf], agent_dir: Option<&Path>) -> (Vec<SessionUsage>, Option<AccountUsage>) {
    let mut files: Vec<PathBuf> = Vec::new();
    for home in homes {
        collect_jsonl(&home.join("sessions"), &mut files);
        collect_jsonl(&home.join("archived_sessions"), &mut files);
    }
    files.sort_by_key(|p| std::cmp::Reverse(file_mtime_epoch(p).unwrap_or(0)));
    files.truncate(MAX_FILES);

    let mut sessions = Vec::new();
    let mut account: Option<AccountUsage> = None;
    for path in &files {
        let Some(info) = parse_rollout(path) else { continue };
        let updated_at = file_mtime_epoch(path).unwrap_or(0);
        if account.is_none()
            && let Some(tc) = &info.token_count
            && (tc.primary.is_some() || tc.secondary.is_some())
        {
            account = Some(AccountUsage {
                kind: AgentKind::Codex,
                primary: tc.primary.clone(),
                secondary: tc.secondary.clone(),
                plan: tc.plan.clone(),
                updated_at,
            });
        }
        if sessions.len() < MAX_SESSIONS
            && let Some(tc) = &info.token_count
        {
            sessions.push(SessionUsage {
                kind: AgentKind::Codex,
                session_id: info.session_id.clone(),
                tab_session_id: None,
                cwd: info.cwd.clone(),
                model: info.model.clone(),
                context: tc.context.clone(),
                state: SessionState::Unknown,
                updated_at,
            });
        }
    }
    if let Some(dir) = agent_dir {
        apply_notify(&mut sessions, dir);
    }
    (sessions, account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_rollout(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    const META: &str = r#"{"timestamp":"2026-10-22T02:00:00.000Z","type":"session_meta","payload":{"id":"019d0000-1111-7222-8333-444455556666","timestamp":"2026-10-22T02:00:00.000Z","cwd":"C:\\work\\demo","originator":"codex_cli_rs","cli_version":"0.50.0","source":"cli","model_provider":"openai"}}"#;
    const TURN: &str = r#"{"timestamp":"2026-10-22T02:00:01.000Z","type":"turn_context","payload":{"turn_id":"t1","cwd":"C:\\work\\demo","model":"gpt-5-codex"}}"#;
    const COUNT: &str = r#"{"timestamp":"2026-10-22T02:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":9000,"cached_input_tokens":4000,"output_tokens":800,"reasoning_output_tokens":300,"total_tokens":9800},"last_token_usage":{"input_tokens":3000,"cached_input_tokens":2000,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":3200},"model_context_window":500000},"rate_limits":{"limit_id":"codex","primary":{"used_percent":42.0,"window_minutes":300,"resets_at":1792700000},"secondary":{"used_percent":17.5,"window_minutes":10080,"resets_at":1793200000},"credits":{"has_credits":false,"unlimited":false,"balance":null},"plan_type":"plus","rate_limit_reached_type":null}}}"#;

    #[test]
    fn parses_rollout_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_rollout(dir.path(), "rollout-2026-10-22T10-00-00-019d0000.jsonl", &[META, TURN, COUNT]);
        let info = parse_rollout(&path).expect("应能解析");
        assert_eq!(info.session_id, "019d0000-1111-7222-8333-444455556666");
        assert_eq!(info.cwd.as_deref(), Some("C:\\work\\demo"));
        assert_eq!(info.model.as_deref(), Some("gpt-5-codex"));
        let tc = info.token_count.expect("应有 token_count");
        let ctx = tc.context.expect("应有上下文");
        assert_eq!(ctx.used_tokens, 3000);
        assert_eq!(ctx.window_tokens, 500000);
        assert_eq!(tc.primary.unwrap().used_percent, 42.0);
        let secondary = tc.secondary.unwrap();
        assert_eq!(secondary.window_minutes, Some(10080));
        assert_eq!(secondary.resets_at, Some(1793200000));
        assert_eq!(tc.plan.as_deref(), Some("plus"));
    }

    #[test]
    fn tolerates_missing_windows_and_fields() {
        // Pro 套餐 primary 缺席、老文件没有 cache_write 字段，都不应失败
        let count = r#"{"timestamp":"2026-10-22T02:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11},"model_context_window":200000},"rate_limits":{"secondary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1793200000},"plan_type":"pro"}}}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = write_rollout(dir.path(), "r.jsonl", &[META, count]);
        let info = parse_rollout(&path).unwrap();
        let tc = info.token_count.unwrap();
        assert!(tc.primary.is_none());
        assert_eq!(tc.secondary.unwrap().used_percent, 3.0);
    }

    #[test]
    fn collect_picks_freshest_account_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions").join("2026").join("10").join("22");
        std::fs::create_dir_all(&sessions).unwrap();
        write_rollout(&sessions, "rollout-a.jsonl", &[META, TURN, COUNT]);
        let (list, account) = collect(&[dir.path().to_path_buf()], None);
        assert_eq!(list.len(), 1);
        let account = account.expect("应有账号用量");
        assert_eq!(account.kind, AgentKind::Codex);
        assert_eq!(account.primary.unwrap().used_percent, 42.0);
    }

    #[test]
    fn notify_marks_session_done_and_links_tab() {
        use crate::spool::{SpoolEntry, codex_notify_file_name, write_entry};
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions").join("2026").join("10").join("22");
        std::fs::create_dir_all(&sessions).unwrap();
        write_rollout(&sessions, "rollout-a.jsonl", &[META, TURN, COUNT]);
        let agent_dir = dir.path().join("agent");
        let entry = SpoolEntry {
            received_at: 4_000_000_000, // 远晚于文件 mtime
            tab_session_id: Some("tab-9".into()),
            payload: serde_json::json!({ "type": "agent-turn-complete", "thread-id": "019d0000-1111-7222-8333-444455556666", "cwd": "C:\\work\\demo" }),
        };
        write_entry(&agent_dir, &codex_notify_file_name("019d0000-1111-7222-8333-444455556666"), &entry).unwrap();
        let (list, _) = collect(&[dir.path().to_path_buf()], Some(&agent_dir));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state, SessionState::Done);
        assert_eq!(list[0].tab_session_id.as_deref(), Some("tab-9"));
        assert_eq!(list[0].updated_at, 4_000_000_000);
    }
}
