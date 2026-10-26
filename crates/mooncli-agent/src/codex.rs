//! Codex 会话文件（rollout JSONL）解析。格式依据 openai/codex 源码核实（2026-10-22）：
//! - 文件在 `$CODEX_HOME/sessions/年/月/日/rollout-<时间>-<thread id>.jsonl`。
//! - 每行是 `{timestamp, type, payload}`；首行 `type = "session_meta"`，payload 带 `id`、`cwd`。
//! - 用量事件是 `type = "event_msg"` 且 `payload.type = "token_count"`：payload 的 `info`
//!   给 token 用量和 `model_context_window`，`rate_limits` 是后端下发、客户端落盘的额度快照
//!   （primary/secondary 各带 used_percent、window_minutes、resets_at epoch 秒、plan_type）。
//!   读它等于读官方 /status 的同源数据，不需要任何凭证。
//!
//! 额度优先用 codex app-server 实时查询（codex_live），会话文件只是没法实时查询时的退路。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Focus;
use crate::jsonl::{collect_jsonl, first_line, tail_lines};
use crate::model::{AccountUsage, AgentKind, ContextUsage, CurrentSession, RateWindow, SnapshotSource};
use crate::spool::{list_entries, read_entry};
use crate::timeutil::file_mtime_epoch;

/// 找账号额度时最多翻几个最近的会话文件（最新的那个通常就有）。
const ACCOUNT_FILES: usize = 8;
/// 尾部扫描的起始/最大范围（字节）。token_count 每轮都有，通常一探即中。
const TAIL_START: u64 = 256 * 1024;
const TAIL_MAX: u64 = 4 * 1024 * 1024;
/// Codex 算上下文剩余时扣掉的固定开销（系统提示词、工具定义），和它界面上显示的百分比对齐。
/// 见 codex-rs/protocol 的 `TokenUsage::percent_of_context_window_remaining`。
const BASELINE_TOKENS: i64 = 12_000;

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
    Some(RateWindow {
        used_percent: v.get("used_percent")?.as_f64()?,
        window_minutes: v.get("window_minutes").and_then(Value::as_i64),
        resets_at: v.get("resets_at").and_then(Value::as_i64),
    })
}

/// 上下文占用：最近一轮的 total_tokens 就是当前上下文里的量（Codex 自己也这么算）。
fn context_usage(info: &Value) -> Option<ContextUsage> {
    let window = info.get("model_context_window")?.as_i64()?;
    let last = info.get("last_token_usage")?;
    let used = last
        .get("total_tokens")
        .and_then(Value::as_i64)
        .or_else(|| last.get("input_tokens").and_then(Value::as_i64))?;
    let mut ctx = ContextUsage::simple(used, window);
    if window > BASELINE_TOKENS {
        let effective = (window - BASELINE_TOKENS) as f64;
        let used_eff = (used - BASELINE_TOKENS).max(0) as f64;
        ctx.used_percent = (used_eff / effective * 100.0).clamp(0.0, 100.0);
    }
    Some(ctx)
}

fn parse_token_count(payload: &Value) -> TokenCount {
    let limits = payload.get("rate_limits");
    TokenCount {
        context: payload.get("info").and_then(context_usage),
        primary: limits.and_then(|l| l.get("primary")).and_then(rate_window),
        secondary: limits.and_then(|l| l.get("secondary")).and_then(rate_window),
        plan: limits.and_then(|l| l.get("plan_type")).and_then(Value::as_str).map(str::to_string),
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
    let meta: Value = serde_json::from_str(&first_line(path)?).ok()?;
    if meta.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let payload = meta.get("payload")?;
    let session_id = payload.get("id")?.as_str()?.to_string();
    let mut cwd = payload.get("cwd").and_then(Value::as_str).map(str::to_string);

    let mut token_count = None;
    let mut model = None;
    let mut tail = TAIL_START;
    loop {
        for line in tail_lines(path, tail).iter().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            match v.get("type").and_then(Value::as_str) {
                Some("event_msg") if token_count.is_none() => {
                    if let Some(p) =
                        v.get("payload").filter(|p| p.get("type").and_then(Value::as_str) == Some("token_count"))
                    {
                        token_count = Some(parse_token_count(p));
                    }
                }
                Some("turn_context") if model.is_none() => {
                    if let Some(p) = v.get("payload") {
                        model = p.get("model").and_then(Value::as_str).map(str::to_string);
                        if let Some(c) = p.get("cwd").and_then(Value::as_str) {
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

/// 所有会话文件，最近改过的在前（带 mtime）。
fn rollouts_by_mtime(homes: &[PathBuf]) -> Vec<(i64, PathBuf)> {
    let mut files = Vec::new();
    for home in homes {
        collect_jsonl(&home.join("sessions"), &mut files);
    }
    let mut out: Vec<(i64, PathBuf)> = files.into_iter().filter_map(|p| file_mtime_epoch(&p).map(|t| (t, p))).collect();
    out.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    out
}

/// 最近几天目录里的会话文件，最近改过的在前。目录按 年/月/日 倒序走，凑够 `limit` 个文件就停，
/// 不扫全部历史（每 5 秒轮询一次也不费事）。被续上的老会话不在最近的目录里，这里不管：
/// 额度是账号级的，任何一个新会话里的快照都一样。
fn recent_rollouts(homes: &[PathBuf], limit: usize) -> Vec<(i64, PathBuf)> {
    fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|e| e.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
            .unwrap_or_default();
        v.sort();
        v.reverse();
        v
    }
    let mut out = Vec::new();
    for home in homes {
        let mut found = 0;
        'outer: for year in sorted_dirs(&home.join("sessions")) {
            for month in sorted_dirs(&year) {
                for day in sorted_dirs(&month) {
                    let mut files = Vec::new();
                    collect_jsonl(&day, &mut files);
                    found += files.len();
                    out.extend(files.into_iter().filter_map(|p| file_mtime_epoch(&p).map(|t| (t, p))));
                    if found >= limit {
                        break 'outer;
                    }
                }
            }
        }
    }
    out.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    out
}

/// 会话文件里最新的额度快照（Codex 每轮回复后落盘，时刻取文件修改时间）。
pub fn latest_account(homes: &[PathBuf]) -> Option<AccountUsage> {
    for (mtime, path) in recent_rollouts(homes, ACCOUNT_FILES).into_iter().take(ACCOUNT_FILES) {
        let Some(tc) = parse_rollout(&path).and_then(|i| i.token_count) else { continue };
        if let Some(a) = AccountUsage::from_windows(
            AgentKind::Codex,
            tc.primary,
            tc.secondary,
            tc.plan,
            mtime,
            SnapshotSource::SessionFile,
        ) {
            return Some(a);
        }
    }
    None
}

fn same_dir(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

/// 当前标签页里正在跑的 Codex 会话的上下文。依次看：
/// 1. 这个标签页送来的 notify（带 thread id，直接对上会话文件）；
/// 2. 本次运行以来写过、目录和标签页一致的最新会话文件。
pub fn current(homes: &[PathBuf], agent_dir: &Path, focus: &Focus) -> CurrentSession {
    let mut cur = CurrentSession { kind: AgentKind::Codex, model: None, context: None, updated_at: focus.since };
    let thread = list_entries(agent_dir, "codex-notify-")
        .iter()
        .filter_map(|p| read_entry(p))
        .filter(|e| e.tab_session_id.as_deref() == Some(focus.tab_id.as_str()) && e.received_at >= focus.since)
        .max_by_key(|e| e.received_at)
        .and_then(|e| e.payload.get("thread-id").and_then(Value::as_str).map(str::to_string));
    let files: Vec<(i64, PathBuf)> =
        rollouts_by_mtime(homes).into_iter().take_while(|(t, _)| *t >= focus.since).collect();
    let by_thread = thread.and_then(|id| {
        files.iter().find(|(_, p)| p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.ends_with(&id))).cloned()
    });
    let found = by_thread.and_then(|(t, p)| parse_rollout(&p).map(|i| (t, i))).or_else(|| {
        let cwd = focus.cwd.as_deref()?;
        files.iter().find_map(|(t, p)| {
            let info = parse_rollout(p)?;
            info.cwd.as_deref().is_some_and(|c| same_dir(c, cwd)).then_some((*t, info))
        })
    });
    if let Some((mtime, info)) = found {
        cur.updated_at = mtime;
        cur.model = info.model;
        cur.context = info.token_count.and_then(|tc| tc.context);
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_rollout(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    const ID: &str = "019d0000-1111-7222-8333-444455556666";
    const META: &str = r#"{"timestamp":"2026-10-22T02:00:00.000Z","type":"session_meta","payload":{"id":"019d0000-1111-7222-8333-444455556666","timestamp":"2026-10-22T02:00:00.000Z","cwd":"C:\\work\\demo","originator":"codex_cli_rs","cli_version":"0.50.0","source":"cli","model_provider":"openai"}}"#;
    const TURN: &str = r#"{"timestamp":"2026-10-22T02:00:01.000Z","type":"turn_context","payload":{"turn_id":"t1","cwd":"C:\\work\\demo","model":"gpt-5-codex"}}"#;
    const COUNT: &str = r#"{"timestamp":"2026-10-22T02:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":90000,"cached_input_tokens":4000,"output_tokens":800,"reasoning_output_tokens":300,"total_tokens":90800},"last_token_usage":{"input_tokens":30000,"cached_input_tokens":2000,"output_tokens":2000,"reasoning_output_tokens":50,"total_tokens":32000},"model_context_window":272000},"rate_limits":{"limit_id":"codex","primary":{"used_percent":42.0,"window_minutes":300,"resets_at":1792700000},"secondary":{"used_percent":17.5,"window_minutes":10080,"resets_at":1793200000},"credits":{"has_credits":false,"unlimited":false,"balance":null},"plan_type":"plus","rate_limit_reached_type":null}}}"#;

    fn sessions(home: &Path) -> PathBuf {
        home.join("sessions").join("2026").join("10").join("22")
    }

    #[test]
    fn parses_rollout_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_rollout(dir.path(), &format!("rollout-2026-10-22T10-00-00-{ID}.jsonl"), &[META, TURN, COUNT]);
        let info = parse_rollout(&path).expect("应能解析");
        assert_eq!(info.session_id, ID);
        assert_eq!(info.cwd.as_deref(), Some("C:\\work\\demo"));
        assert_eq!(info.model.as_deref(), Some("gpt-5-codex"));
        let tc = info.token_count.expect("应有 token_count");
        let ctx = tc.context.expect("应有上下文");
        assert_eq!((ctx.used_tokens, ctx.window_tokens), (32000, 272000));
        // 和 Codex 自己的口径一致：(32000-12000)/(272000-12000)
        assert!((ctx.used_percent - 7.69).abs() < 0.01);
        assert_eq!(tc.primary.unwrap().used_percent, 42.0);
        assert_eq!(tc.secondary.unwrap().resets_at, Some(1793200000));
        assert_eq!(tc.plan.as_deref(), Some("plus"));
    }

    #[test]
    fn tolerates_missing_windows() {
        // Pro 套餐 primary 缺席，只有周窗口
        let count = r#"{"timestamp":"2026-10-22T02:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"secondary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1793200000},"plan_type":"pro"}}}"#;
        let dir = tempfile::tempdir().unwrap();
        write_rollout(&sessions(dir.path()), "r.jsonl", &[META, count]);
        let a = latest_account(&[dir.path().to_path_buf()]).unwrap();
        assert!(a.five_hour.is_none());
        assert_eq!(a.seven_day.unwrap().used_percent, 3.0);
        assert_eq!(a.plan.as_deref(), Some("pro"));
    }

    #[test]
    fn current_finds_session_by_notify_thread_or_cwd() {
        use crate::spool::{SpoolEntry, codex_notify_file_name, write_entry};
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        write_rollout(&sessions(&home), &format!("rollout-2026-10-22T10-00-00-{ID}.jsonl"), &[META, TURN, COUNT]);
        let agent = dir.path().join("agent");
        let focus = |cwd: Option<&str>, since: i64| Focus {
            kind: AgentKind::Codex,
            tab_id: "tab-9".into(),
            cwd: cwd.map(str::to_string),
            since,
        };
        // 按目录（大小写、斜杠不同也算同一个）
        let cur = current(std::slice::from_ref(&home), &agent, &focus(Some("c:/work/demo/"), 0));
        assert_eq!(cur.context.unwrap().used_tokens, 32000);
        assert_eq!(cur.model.as_deref(), Some("gpt-5-codex"));
        assert!(current(std::slice::from_ref(&home), &agent, &focus(Some(r"C:\other"), 0)).context.is_none());
        // 运行开始之后没写过的文件不算
        assert!(
            current(std::slice::from_ref(&home), &agent, &focus(Some(r"C:\work\demo"), i64::MAX)).context.is_none()
        );
        // notify 带来的 thread id 优先，不看目录
        write_entry(
            &agent,
            &codex_notify_file_name(ID),
            &SpoolEntry {
                received_at: 5,
                tab_session_id: Some("tab-9".into()),
                payload: serde_json::json!({ "type": "agent-turn-complete", "thread-id": ID }),
            },
        )
        .unwrap();
        assert!(current(std::slice::from_ref(&home), &agent, &focus(None, 0)).context.is_some());
    }
}
