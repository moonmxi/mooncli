//! 完成提醒（docs/07 第 7 节）：Claude Code 一轮结束、要你确认，Codex 一轮结束时，弹 Windows 系统通知。
//!
//! 事件来自官方扩展点：Claude Code 的 Stop / Notification hooks、Codex 的 notify（agent-turn-complete）。
//! mooncli-bridge 收到后分两种情况：
//! - Agent 跑在 mooncli 的标签页里（祖先进程里有 mooncli）：把提醒写进 `alerts/` 交给主程序。
//!   主程序知道你是不是正看着那个标签页，看着就不打扰；没看着才弹通知，点通知切到那个标签页。
//! - 跑在别的终端里：bridge 自己弹通知；你正看着那个终端窗口时不弹（可在设置里关掉这个判断）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::AgentKind;

/// 通知用的 AppUserModelID，和安装包、Tauri 的 identifier 一致，通知上显示 mooncli 的名字和图标。
pub const APP_ID: &str = "dev.moonmxi.mooncli";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertEvent {
    /// 一轮结束
    Done,
    /// 在等你确认（要权限、要你回答）
    Waiting,
}

/// 一条提醒。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub kind: AgentKind,
    pub event: AlertEvent,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_session_id: Option<String>,
    pub at: i64,
}

/// 提醒正文里的一句摘要：取回复的头几行，去掉 Markdown 记号、压成一行，太长截断。
pub fn summarize(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['#', '>', '-', '*', ' ']).trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&line.replace(['*', '`'], ""));
        if out.chars().count() >= max_chars {
            break;
        }
    }
    if out.chars().count() > max_chars {
        out = out.chars().take(max_chars - 1).collect::<String>() + "…";
    }
    out
}

fn folder(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?.trim_end_matches(['\\', '/']);
    cwd.rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).map(str::to_string)
}

/// 正文：`目录 · 摘要`，缺哪样省哪样。
fn body(cwd: Option<&str>, text: Option<&str>) -> String {
    let summary = text.map(|t| summarize(t, 110)).filter(|s| !s.is_empty());
    match (folder(cwd), summary) {
        (Some(f), Some(s)) => format!("{f} · {s}"),
        (Some(f), None) => f,
        (None, Some(s)) => s,
        (None, None) => String::new(),
    }
}

/// Claude Code 的 hook 事件 → 提醒。`last_text` 在需要时读转录里最后一条回复（Stop 的输入里不一定有）。
pub fn from_claude_hook(payload: &Value, last_text: impl FnOnce() -> Option<String>, now: i64) -> Option<Alert> {
    let cwd = payload.get("cwd").and_then(Value::as_str);
    let (event, title, text) = match payload.get("hook_event_name").and_then(Value::as_str)? {
        "Stop" => {
            let text =
                payload.get("last_assistant_message").and_then(Value::as_str).map(str::to_string).or_else(last_text);
            (AlertEvent::Done, "Claude Code 完成了", text)
        }
        "Notification" => {
            let message = payload.get("message").and_then(Value::as_str).unwrap_or("");
            match payload.get("notification_type").and_then(Value::as_str) {
                Some("permission_prompt") | Some("elicitation_dialog") => {}
                // 空闲提醒是在一轮结束后又等了一阵才发，Stop 已经提醒过了；登录成功之类与你无关
                Some(_) => return None,
                // 老版本没有 notification_type，按文字区分
                None if message.contains("waiting for your input") => return None,
                None => {}
            }
            (AlertEvent::Waiting, "Claude Code 需要你确认", Some(message.to_string()))
        }
        _ => return None,
    };
    Some(Alert {
        kind: AgentKind::Claude,
        event,
        title: title.into(),
        body: body(cwd, text.as_deref()),
        tab_session_id: None,
        at: now,
    })
}

/// Codex notify 的事件 → 提醒。
pub fn from_codex(payload: &Value, now: i64) -> Option<Alert> {
    let cwd = payload.get("cwd").and_then(Value::as_str);
    let (event, title, text) = match payload.get("type").and_then(Value::as_str)? {
        "agent-turn-complete" => {
            (AlertEvent::Done, "Codex 完成了", payload.get("last-assistant-message").and_then(Value::as_str))
        }
        // 新版本可能加的审批事件
        t if t.contains("approval") => (AlertEvent::Waiting, "Codex 需要你确认", None),
        _ => return None,
    };
    Some(Alert {
        kind: AgentKind::Codex,
        event,
        title: title.into(),
        body: body(cwd, text),
        tab_session_id: None,
        at: now,
    })
}

// ---------- 交给主程序 ----------

pub fn alerts_dir(agent_dir: &Path) -> PathBuf {
    agent_dir.join("alerts")
}

/// bridge 把提醒交给主程序：一条一个文件，原子写入。
pub fn write_alert(agent_dir: &Path, alert: &Alert) -> std::io::Result<()> {
    let dir = alerts_dir(agent_dir);
    std::fs::create_dir_all(&dir)?;
    let name = format!("{}-{}.json", alert.at, std::process::id());
    let tmp = dir.join(format!(".tmp-{name}"));
    std::fs::write(&tmp, serde_json::to_vec(alert)?)?;
    std::fs::rename(&tmp, dir.join(name))
}

/// 主程序取走所有待发的提醒（读完即删），按时间先后。
pub fn take_alerts(agent_dir: &Path) -> Vec<Alert> {
    let Ok(entries) = std::fs::read_dir(alerts_dir(agent_dir)) else { return Vec::new() };
    let mut out = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with(".tmp-") || !name.ends_with(".json") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(alert) = serde_json::from_slice::<Alert>(&bytes)
        {
            out.push(alert);
        }
        let _ = std::fs::remove_file(&path);
    }
    out.sort_by_key(|a| a.at);
    out
}

// ---------- Windows 系统通知 ----------

/// 弹一条系统通知。`on_click` 在用户点通知时调用（只在本进程还活着时有效）。
#[cfg(windows)]
pub fn show_toast(title: &str, body: &str, on_click: Option<Box<dyn Fn() + Send>>) -> Result<(), String> {
    use tauri_winrt_notification::{Duration, Toast};
    let mut toast = Toast::new(APP_ID).title(title).duration(Duration::Short);
    if !body.is_empty() {
        toast = toast.text1(body);
    }
    if let Some(f) = on_click {
        toast = toast.on_activated(move |_| {
            f();
            Ok(())
        });
    }
    toast.show().map_err(|e| format!("系统通知发送失败：{e}"))
}

#[cfg(not(windows))]
pub fn show_toast(_title: &str, _body: &str, _on_click: Option<Box<dyn Fn() + Send>>) -> Result<(), String> {
    Err("系统通知目前只支持 Windows".into())
}

/// 在当前用户下登记 AppUserModelID（名字 + 图标），没有开始菜单快捷方式（开发版、便携版）时
/// 通知也能显示 mooncli 的名字和图标。只写 HKCU\Software\Classes\AppUserModelId\<APP_ID>。
#[cfg(windows)]
pub fn register_app_id(icon: Option<&Path>) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let subkey = wide(&format!(r"Software\Classes\AppUserModelId\{APP_ID}"));
    let mut values = vec![("DisplayName", "mooncli".to_string())];
    if let Some(icon) = icon {
        values.push(("IconUri", icon.display().to_string()));
    }
    for (name, value) in values {
        let name = wide(name);
        let data = wide(&value);
        // SAFETY: 参数都是以 0 结尾的 UTF-16 字符串；子键不存在时 RegSetKeyValueW 会建好
        let rc = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if rc != 0 {
            return Err(format!("登记通知身份失败（错误码 {rc}）"));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn register_app_id(_icon: Option<&Path>) -> Result<(), String> {
    Ok(())
}

/// 本进程的祖先进程（父、祖父……），每项是 (pid, 可执行文件名小写)。
#[cfg(windows)]
pub fn ancestors() -> Vec<(u32, String)> {
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    let mut table: HashMap<u32, (u32, String)> = HashMap::new();
    // SAFETY: 标准的进程快照遍历，句柄用完关闭
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            table.insert(e.th32ProcessID, (e.th32ParentProcessID, name));
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    let mut out = Vec::new();
    let mut pid = std::process::id();
    while let Some((parent, _)) = table.get(&pid) {
        let parent = *parent;
        if parent == 0 || out.iter().any(|(p, _)| *p == parent) || out.len() > 32 {
            break;
        }
        match table.get(&parent) {
            Some((_, name)) => out.push((parent, name.clone())),
            None => break,
        }
        pid = parent;
    }
    out
}

#[cfg(not(windows))]
pub fn ancestors() -> Vec<(u32, String)> {
    Vec::new()
}

/// 前台窗口是不是某个祖先进程的（也就是你正看着跑这个 Agent 的终端）。
#[cfg(windows)]
pub fn foreground_is_ancestor(ancestors: &[(u32, String)]) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    // SAFETY: 只读查询
    let pid = unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        pid
    };
    pid != 0 && ancestors.iter().any(|(p, _)| *p == pid)
}

#[cfg(not(windows))]
pub fn foreground_is_ancestor(_ancestors: &[(u32, String)]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_stop_uses_last_message_or_transcript() {
        let p = json!({ "hook_event_name": "Stop", "cwd": r"D:\work\app", "session_id": "s" });
        let a = from_claude_hook(&p, || Some("## 完成\n\n改好了 `foo.rs`，**测试**都过了。".into()), 7).unwrap();
        assert_eq!(a.event, AlertEvent::Done);
        assert_eq!(a.title, "Claude Code 完成了");
        assert_eq!(a.body, "app · 完成 改好了 foo.rs，测试都过了。");
        assert_eq!(a.at, 7);
        let p = json!({ "hook_event_name": "Stop", "last_assistant_message": "好了" });
        assert_eq!(from_claude_hook(&p, || panic!("有现成的就不读转录"), 0).unwrap().body, "好了");
    }

    #[test]
    fn claude_notifications_only_when_you_are_needed() {
        let perm = json!({ "hook_event_name": "Notification", "notification_type": "permission_prompt",
            "message": "Claude needs your permission to use Bash", "cwd": "/w/x" });
        let a = from_claude_hook(&perm, || None, 0).unwrap();
        assert_eq!((a.event, a.body.as_str()), (AlertEvent::Waiting, "x · Claude needs your permission to use Bash"));
        let idle = json!({ "hook_event_name": "Notification", "notification_type": "idle_prompt", "message": "Claude is waiting for your input" });
        assert!(from_claude_hook(&idle, || None, 0).is_none());
        let old_idle = json!({ "hook_event_name": "Notification", "message": "Claude is waiting for your input" });
        assert!(from_claude_hook(&old_idle, || None, 0).is_none());
        assert!(from_claude_hook(&json!({ "hook_event_name": "UserPromptSubmit" }), || None, 0).is_none());
    }

    #[test]
    fn codex_turn_complete() {
        let p = json!({ "type": "agent-turn-complete", "thread-id": "t", "turn-id": "1", "cwd": "C:\\w\\demo\\",
            "input-messages": ["fix"], "last-assistant-message": "Fixed the bug." });
        let a = from_codex(&p, 3).unwrap();
        assert_eq!(
            (a.kind, a.event, a.title.as_str(), a.body.as_str()),
            (AgentKind::Codex, AlertEvent::Done, "Codex 完成了", "demo · Fixed the bug.")
        );
        assert!(from_codex(&json!({ "type": "other" }), 0).is_none());
    }

    #[test]
    fn summarize_truncates() {
        let long = "字".repeat(300);
        let s = summarize(&long, 20);
        assert_eq!(s.chars().count(), 20);
        assert!(s.ends_with('…'));
        assert_eq!(summarize("```rust\ncode\n```\n", 50), "code");
    }

    #[test]
    fn alerts_round_trip_through_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = from_codex(&json!({ "type": "agent-turn-complete" }), 2).unwrap();
        a.tab_session_id = Some("tab-1".into());
        write_alert(dir.path(), &a).unwrap();
        let got = take_alerts(dir.path());
        assert_eq!(got, vec![a]);
        assert!(take_alerts(dir.path()).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn ancestors_include_parent() {
        let a = ancestors();
        assert!(!a.is_empty(), "测试进程至少有个父进程（cargo）");
    }
}
