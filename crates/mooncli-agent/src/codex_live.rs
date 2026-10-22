//! 实时查询 Codex 账号额度：通过官方客户端自己的 `codex app-server`（JSON-RPC over stdio，
//! Codex 的 IDE 插件用的就是它）调 `account/rateLimits/read`。
//!
//! 合规边界（docs/07 第 1 节）：mooncli 不读凭证、不自己发网络请求——登录与联网全由用户
//! 本机已登录的 Codex 自己完成，我们只是它公开协议的一个客户端。和读会话文件相比，
//! 好处是不用等下一轮对话就能拿到最新额度（会话文件只在模型响应时才落盘快照）。
//!
//! 协议依据：`codex app-server generate-json-schema` 导出的 v2 协议
//! （GetAccountRateLimitsResponse / RateLimitSnapshot / RateLimitWindow），2026-10 核实。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::model::{AccountUsage, AgentKind, RateWindow, SnapshotSource};
use crate::timeutil::now_epoch;

/// 找 Codex 的原生可执行文件。npm 装的 Codex 在 PATH 里只有 codex.cmd 外壳（要经 node 转一手），
/// 这里顺着外壳找到包里自带的 codex.exe，直接起它更快也不会闪出控制台窗口。
pub fn find_codex_exe(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let exe = dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        if exe.is_file() {
            return Some(exe);
        }
        if cfg!(windows)
            && dir.join("codex.cmd").is_file()
            && let Some(native) = npm_native_codex(&dir)
        {
            return Some(native);
        }
    }
    None
}

/// npm 全局目录下 @openai/codex 自带的原生程序：
/// `node_modules/@openai/codex/node_modules/@openai/codex-win32-<arch>/vendor/<triple>/bin/codex.exe`
/// （依赖被提升时少一层 `@openai/codex/node_modules`）。
fn npm_native_codex(npm_dir: &Path) -> Option<PathBuf> {
    let base = npm_dir.join("node_modules").join("@openai");
    let roots = [base.join("codex").join("node_modules").join("@openai"), base.clone()];
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for pkg in entries.flatten() {
            if !pkg.file_name().to_string_lossy().starts_with("codex-win32-") {
                continue;
            }
            let Ok(triples) = std::fs::read_dir(pkg.path().join("vendor")) else { continue };
            for t in triples.flatten() {
                let exe = t.path().join("bin").join("codex.exe");
                if exe.is_file() {
                    return Some(exe);
                }
                let exe = t.path().join("codex").join("codex.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    None
}

fn window(v: &Value) -> Option<RateWindow> {
    let used = v.get("usedPercent")?.as_f64()?;
    Some(RateWindow {
        used_percent: used,
        window_minutes: v.get("windowDurationMins").and_then(Value::as_i64),
        resets_at: v.get("resetsAt").and_then(Value::as_i64),
    })
}

/// 解析 account/rateLimits/read 的 result。优先多桶视图里的 `codex` 桶，没有就用兼容视图。
pub fn parse_rate_limits(result: &Value, now: i64) -> Option<AccountUsage> {
    let snap = result
        .get("rateLimitsByLimitId")
        .and_then(|m| m.get("codex"))
        .filter(|v| v.is_object())
        .or_else(|| result.get("rateLimits"))?;
    AccountUsage::from_windows(
        AgentKind::Codex,
        snap.get("primary").and_then(window),
        snap.get("secondary").and_then(window),
        snap.get("planType").and_then(Value::as_str).filter(|p| *p != "unknown").map(str::to_string),
        now,
        SnapshotSource::Live,
    )
}

/// 起一个 `codex app-server`，握手后读一次额度就关掉。拿不到（没装、没登录、API key 登录
/// 没有额度窗口、超时）一律返回 Err，调用方退回读会话文件。
pub fn fetch(exe: &Path, timeout: Duration) -> Result<AccountUsage, String> {
    let mut cmd = Command::new(exe);
    cmd.arg("app-server").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("启动 codex app-server 失败：{e}"))?;
    let result = talk(&mut child, timeout);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn talk(child: &mut std::process::Child, timeout: Duration) -> Result<AccountUsage, String> {
    let deadline = Instant::now() + timeout;
    let stdout = child.stdout.take().ok_or("拿不到 stdout")?;
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut stdin = child.stdin.take().ok_or("拿不到 stdin")?;
    let mut send = |v: Value| -> Result<(), String> {
        writeln!(stdin, "{v}").and_then(|_| stdin.flush()).map_err(|e| format!("写入 app-server 失败：{e}"))
    };
    let version = env!("CARGO_PKG_VERSION");
    send(
        json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "mooncli", "title": "mooncli", "version": version } } }),
    )?;
    wait_for(&rx, 1, deadline)?;
    send(json!({ "method": "initialized" }))?;
    send(json!({ "id": 2, "method": "account/rateLimits/read" }))?;
    let result = wait_for(&rx, 2, deadline)?;
    parse_rate_limits(&result, now_epoch()).ok_or_else(|| "这个账号没有额度窗口（可能是 API key 登录）".into())
}

/// 等某个 id 的响应，跳过期间的通知。
fn wait_for(rx: &mpsc::Receiver<String>, id: i64, deadline: Instant) -> Result<Value, String> {
    loop {
        let left = deadline.checked_duration_since(Instant::now()).ok_or("codex app-server 响应超时")?;
        let line = rx.recv_timeout(left).map_err(|_| "codex app-server 响应超时或已退出".to_string())?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if v.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(err) = v.get("error") {
            let msg = err.get("message").and_then(Value::as_str).unwrap_or("未知错误");
            return Err(format!("codex app-server 返回错误：{msg}"));
        }
        return v.get("result").cloned().ok_or_else(|| "codex app-server 响应里没有 result".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_response_shape() {
        // 2026-10-22 用 codex-cli 0.144.6 实测的响应（去掉了与额度无关的字段）
        let result = json!({
            "rateLimits": { "limitId": "codex", "primary": { "usedPercent": 3, "windowDurationMins": 300, "resetsAt": 1792694001 },
                "secondary": { "usedPercent": 12, "windowDurationMins": 10080, "resetsAt": 1793280801 }, "planType": "plus" },
            "rateLimitsByLimitId": { "codex": { "limitId": "codex", "primary": { "usedPercent": 3, "windowDurationMins": 300, "resetsAt": 1792694001 },
                "secondary": { "usedPercent": 12, "windowDurationMins": 10080, "resetsAt": 1793280801 }, "planType": "plus" } }
        });
        let a = parse_rate_limits(&result, 100).unwrap();
        assert_eq!(a.kind, AgentKind::Codex);
        assert_eq!(a.source, SnapshotSource::Live);
        assert_eq!(a.five_hour.as_ref().unwrap().used_percent, 3.0);
        assert_eq!(a.seven_day.as_ref().unwrap().window_minutes, Some(10080));
        assert_eq!(a.plan.as_deref(), Some("plus"));
        assert_eq!(a.updated_at, 100);
    }

    #[test]
    fn falls_back_to_single_bucket_and_rejects_empty() {
        let result = json!({ "rateLimits": { "secondary": { "usedPercent": 40, "windowDurationMins": 10080 }, "planType": "unknown" } });
        let a = parse_rate_limits(&result, 1).unwrap();
        assert!(a.five_hour.is_none());
        assert_eq!(a.seven_day.unwrap().used_percent, 40.0);
        assert!(a.plan.is_none());
        assert!(parse_rate_limits(&json!({ "rateLimits": { "primary": null } }), 1).is_none());
    }
}
