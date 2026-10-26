//! 直接问 Claude Code 自己要账号额度：`claude -p /usage --no-session-persistence --output-format json`。
//!
//! `/usage` 是 Claude Code 的本地命令（不调模型、不算一轮对话、不花额度），由 Claude Code 自己去查订阅额度，
//! 结果是给人看的几行文字：
//!
//! ```text
//! Current session: 88% used · resets Oct 26, 2:10pm (Asia/Shanghai)
//! Current week (all models): 19% used · resets Oct 31, 3am (Asia/Shanghai)
//! ```
//!
//! 和 statusLine 相比，好处是不用等你在终端里跑一轮 Claude Code（statusLine 只在终端界面刷新状态栏时调用，
//! 桌面版、IDE 插件里不会触发）。`--no-session-persistence` 让它不留下会话记录，实测也不会触发 hooks。
//! 合规边界同 docs/07 第 1 节：登录和联网都由 Claude Code 完成，mooncli 只读它打印的结果。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveTime, TimeZone};
use serde_json::Value;

use crate::model::{AccountUsage, AgentKind, RateWindow, SnapshotSource};
use crate::timeutil::now_epoch;

/// 找 Claude Code：设置里给的位置 → PATH 里的 claude.exe / claude.cmd → 官方安装脚本的默认位置。
pub fn find_claude_exe(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in names {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let home = std::env::home_dir()?;
    let p = home.join(".local").join("bin").join(if cfg!(windows) { "claude.exe" } else { "claude" });
    p.is_file().then_some(p)
}

/// 跑一次 `/usage`，解析出 5 小时与 7 天额度。没登录订阅（API key）、超时、格式认不出都返回 Err。
pub fn fetch(exe: &Path, work_dir: &Path, timeout: Duration) -> Result<AccountUsage, String> {
    let args = ["-p", "/usage", "--no-session-persistence", "--output-format", "json"];
    let is_cmd = exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
    let mut cmd = if is_cmd {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(exe).args(args);
        c
    } else {
        let mut c = Command::new(exe);
        c.args(args);
        c
    };
    // 不带 mooncli 标签页的身份：就算以后 /usage 触发了 hooks，也不会被当成某个标签页里的会话
    cmd.env_remove("MOONCLI_SESSION_ID").env_remove("CLAUDECODE");
    let _ = std::fs::create_dir_all(work_dir);
    cmd.current_dir(work_dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = cmd.spawn().map_err(|e| format!("启动 claude 失败：{e}"))?;
    let id = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = match rx.recv_timeout(timeout) {
        Ok(r) => r.map_err(|e| format!("读取 claude 输出失败：{e}"))?,
        Err(_) => {
            kill_pid(id);
            return Err("claude /usage 响应超时".into());
        }
    };
    // 原样留一份最近的输出，额度对不上时可以打开看 Claude Code 实际说了什么
    let _ = std::fs::write(work_dir.join("last-usage.json"), &output.stdout);
    let v: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "claude /usage 的输出不是 JSON（版本太旧？）".to_string())?;
    let text = v.get("result").and_then(Value::as_str).unwrap_or("");
    if v.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(format!("claude /usage 出错：{}", first_line(text)));
    }
    parse_usage(text, now_epoch()).ok_or_else(|| explain(text))
}

/// 没解析出额度时，把 Claude Code 实际说的那句话带出来（常见的是额度接口暂时限流、拿不到数据）
fn explain(text: &str) -> String {
    if text.contains("API Usage Billing") || text.contains("API key") {
        return "这个账号用的是 API key，没有订阅额度".into();
    }
    let said = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("You are currently using"));
    match said {
        Some(s) => format!("Claude Code 这次没给额度：{}", s.chars().take(80).collect::<String>()),
        None => "Claude Code 这次没有给出额度，稍后会再查".into(),
    }
}

fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}

fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
}

/// 解析 `/usage` 的文字。`now` 用来推断重置时刻的年份（文字里没写年）。
pub fn parse_usage(text: &str, now: i64) -> Option<AccountUsage> {
    let mut five_hour = None;
    let mut seven_day = None;
    let mut any_week = None;
    for line in text.lines().map(str::trim) {
        let Some((label, rest)) = line.split_once(':') else { continue };
        let label = label.trim().to_ascii_lowercase();
        let Some(window) = parse_window(rest, now) else { continue };
        if label.starts_with("current session") {
            five_hour.get_or_insert(RateWindow { window_minutes: Some(300), ..window });
        } else if label.starts_with("current week") {
            let w = RateWindow { window_minutes: Some(10080), ..window };
            // 「所有模型」的周额度才是总额度；只有分模型的行时退而用第一行
            if label.contains("all models") {
                seven_day = Some(w);
            } else {
                any_week.get_or_insert(w);
            }
        }
    }
    AccountUsage::from_windows(AgentKind::Claude, five_hour, seven_day.or(any_week), None, now, SnapshotSource::Live)
}

/// ` 88% used · resets Oct 26, 2:10pm (Asia/Shanghai)` → 窗口
fn parse_window(rest: &str, now: i64) -> Option<RateWindow> {
    let (pct, after) = rest.split_once('%')?;
    let used_percent: f64 = pct.trim().parse().ok()?;
    let resets_at = after.split_once("resets").and_then(|(_, r)| parse_reset(r, now));
    Some(RateWindow { used_percent, window_minutes: None, resets_at })
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// `Oct 26, 2:10pm (Asia/Shanghai)`、`2:10pm`、`Oct 31, 3am`、`Oct 31` → epoch 秒（按本机时区）。
/// 文字里没有年份：取离现在最近、且不早于两天前的那一年；只有时刻时取下一个这个时刻。
pub fn parse_reset(s: &str, now: i64) -> Option<i64> {
    let s = s.split('(').next()?.trim().trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let now_local = Local.timestamp_opt(now, 0).single()?;
    let mut date: Option<(u32, u32)> = None;
    let mut time = NaiveTime::from_hms_opt(0, 0, 0)?;
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(t) = parse_clock(part) {
            time = t;
        } else {
            let mut words = part.split_whitespace();
            let month = words.next()?.to_ascii_lowercase();
            let m = MONTHS.iter().position(|x| month.starts_with(x))? as u32 + 1;
            let d: u32 = words.next()?.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?;
            date = Some((m, d));
        }
    }
    let to_epoch = |d: NaiveDate| Local.from_local_datetime(&d.and_time(time)).earliest().map(|t| t.timestamp());
    match date {
        Some((m, d)) => {
            let this_year = to_epoch(NaiveDate::from_ymd_opt(now_local.year(), m, d)?)?;
            if this_year < now - 2 * 86_400 {
                to_epoch(NaiveDate::from_ymd_opt(now_local.year() + 1, m, d)?)
            } else {
                Some(this_year)
            }
        }
        None => {
            let today = to_epoch(now_local.date_naive())?;
            Some(if today < now - 60 { today + 86_400 } else { today })
        }
    }
}

/// `2:10pm`、`3am`、`14:00` → 时刻
fn parse_clock(s: &str) -> Option<NaiveTime> {
    let s = s.trim().to_ascii_lowercase();
    let (body, pm) = if let Some(b) = s.strip_suffix("pm") {
        (b.trim(), Some(true))
    } else if let Some(b) = s.strip_suffix("am") {
        (b.trim(), Some(false))
    } else {
        (s.as_str(), None)
    };
    if !body.chars().next()?.is_ascii_digit() {
        return None;
    }
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if pm.is_some() => (body.parse::<u32>().ok()?, 0),
        None => return None,
    };
    let h = match pm {
        Some(true) if h < 12 => h + 12,
        Some(false) if h == 12 => 0,
        _ => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 88% used · resets Oct 26, 2:10pm (Asia/Shanghai)\nCurrent week (all models): 19% used · resets Oct 31, 3am (Asia/Shanghai)\n\nWhat's contributing to your limits usage?\nLast 24h · 316 requests · 6 sessions\n  91%  Start fresh when you switch tasks · run /clear\n";

    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).earliest().unwrap().timestamp()
    }

    #[test]
    fn parses_real_output() {
        let now = local(2026, 10, 26, 12, 40);
        let a = parse_usage(SAMPLE, now).unwrap();
        assert_eq!(a.kind, AgentKind::Claude);
        assert_eq!(a.source, SnapshotSource::Live);
        let five = a.five_hour.unwrap();
        assert_eq!(
            (five.used_percent, five.window_minutes, five.resets_at),
            (88.0, Some(300), Some(local(2026, 10, 26, 14, 10)))
        );
        let week = a.seven_day.unwrap();
        assert_eq!((week.used_percent, week.resets_at), (19.0, Some(local(2026, 10, 31, 3, 0))));
        // 下面「贡献」部分里的百分比不能被当成额度
        assert!(parse_usage("Last 24h\n  91%  Start fresh", now).is_none());
    }

    #[test]
    fn reset_times_without_year_or_date() {
        let now = local(2026, 12, 30, 22, 0);
        assert_eq!(parse_reset(" Jan 2, 3am (UTC)", now), Some(local(2027, 1, 2, 3, 0)));
        assert_eq!(parse_reset("11pm", now), Some(local(2026, 12, 30, 23, 0)));
        assert_eq!(parse_reset("9:30pm", now), Some(local(2026, 12, 31, 21, 30)));
        assert_eq!(parse_reset("Dec 30, 12am", now), Some(local(2026, 12, 30, 0, 0)));
        assert_eq!(parse_reset("Dec 31", now), Some(local(2026, 12, 31, 0, 0)));
        assert_eq!(parse_reset("someday", now), None);
    }

    #[test]
    fn explains_what_claude_said() {
        assert_eq!(
            explain(
                "You are currently using your subscription\n\nUsage data is temporarily unavailable. Try again later."
            ),
            "Claude Code 这次没给额度：Usage data is temporarily unavailable. Try again later."
        );
        assert_eq!(explain(""), "Claude Code 这次没有给出额度，稍后会再查");
    }

    #[test]
    fn per_model_week_is_a_fallback() {
        let now = local(2026, 10, 26, 12, 0);
        let a = parse_usage("Current week (Sonnet only): 7% used · resets Oct 31, 3am", now).unwrap();
        assert!(a.five_hour.is_none());
        assert_eq!(a.seven_day.unwrap().used_percent, 7.0);
        let a = parse_usage("Current week (Sonnet only): 7% used\nCurrent week (all models): 19% used", now).unwrap();
        assert_eq!(a.seven_day.unwrap().used_percent, 19.0);
    }
}
