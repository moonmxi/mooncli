//! 一键接入的纯逻辑：把 mooncli-bridge 写进 Claude Code 的 settings.json 和 Codex 的
//! config.toml，以及精确撤销。只做数据变换不碰文件——备份、落盘、确认都在主进程和界面侧。
//!
//! 原则（docs/07 第 3 节）：
//! - 用户原来的 statusLine 命令保存进 bridge-config（previousStatusLine），bridge 运行时转调；
//! - hooks 是并列追加，不覆盖用户已有的同事件 hooks；撤销时只删 mooncli 加的那几条；
//! - Codex 的 notify 若已有，保存为 previousCodexNotify 由 bridge 转调，再换成 bridge。

use serde_json::{json, Map, Value};

/// bridge 在配置里写下的命令形态：statusLine / hook 都是 `"<bridge 路径>" <子命令>`。
pub fn bridge_command(bridge_path: &str, sub: &str) -> String {
    format!("\"{bridge_path}\" {sub}")
}

/// 这个命令是不是 mooncli-bridge 的（接入过）。
fn is_bridge_command(cmd: &str, sub: &str) -> bool {
    cmd.contains("mooncli-bridge") && cmd.trim_end().ends_with(sub)
}

/// 要接入的 hook 事件（docs/07 第 3 节）。
pub const HOOK_EVENTS: [&str; 6] = ["Notification", "Stop", "SessionStart", "SessionEnd", "UserPromptSubmit", "PostToolUse"];

/// 接入结果：给界面预览/提示用的人话变更清单。
pub struct MergeOutcome {
    pub settings: Value,
    pub changes: Vec<String>,
    /// 用户原来的 statusLine 命令（要存进 bridge-config 让 bridge 转调）；本来就是 bridge 或没有则为 None
    pub previous_statusline: Option<String>,
}

/// 把 statusLine + hooks 接入一份 Claude Code 设置。settings 必须是 JSON 对象。
pub fn claude_integrate(settings: &Value, bridge_path: &str) -> Result<MergeOutcome, String> {
    let mut root = settings.as_object().cloned().ok_or("settings.json 的顶层不是对象")?;
    let mut changes = Vec::new();
    let mut previous_statusline = None;

    // statusLine
    let want = bridge_command(bridge_path, "statusline");
    match root.get("statusLine") {
        Some(existing) => {
            let cur = existing.get("command").and_then(|c| c.as_str()).unwrap_or("");
            if is_bridge_command(cur, "statusline") {
                changes.push("statusLine 已是 mooncli-bridge，不变".into());
            } else {
                if !cur.is_empty() {
                    previous_statusline = Some(cur.to_string());
                    changes.push(format!("statusLine 原命令「{cur}」将保存并由 bridge 转调，显示效果不变"));
                } else {
                    changes.push("statusLine 原为空对象，将设为 mooncli-bridge".into());
                }
                root.insert("statusLine".into(), json!({ "type": "command", "command": want }));
                changes.push("statusLine 改为 mooncli-bridge statusline".into());
            }
        }
        None => {
            root.insert("statusLine".into(), json!({ "type": "command", "command": want }));
            changes.push("新增 statusLine：mooncli-bridge statusline".into());
        }
    }

    // hooks：并列追加
    let hook_cmd = bridge_command(bridge_path, "hook");
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks_obj = hooks.as_object_mut().ok_or("settings.json 的 hooks 不是对象")?;
    for event in HOOK_EVENTS {
        let entry = json!({ "hooks": [{ "type": "command", "command": hook_cmd }] });
        match hooks_obj.get_mut(event) {
            Some(list) => {
                let arr = list.as_array_mut().ok_or(format!("hooks.{event} 不是数组"))?;
                let already = arr.iter().any(|e| {
                    e.get("hooks")
                        .and_then(|h| h.as_array())
                        .is_some_and(|hs| hs.iter().any(|h| h.get("command").and_then(|c| c.as_str()).is_some_and(|c| is_bridge_command(c, "hook"))))
                });
                if already {
                    changes.push(format!("hooks.{event} 已接入，不变"));
                } else {
                    arr.push(entry);
                    changes.push(format!("hooks.{event} 追加 mooncli-bridge hook"));
                }
            }
            None => {
                hooks_obj.insert(event.into(), json!([entry]));
                changes.push(format!("hooks.{event} 新增 mooncli-bridge hook"));
            }
        }
    }

    Ok(MergeOutcome { settings: Value::Object(root), changes, previous_statusline })
}

/// 撤销接入：statusLine 若当前是 bridge 则删掉（原命令由界面按 bridge-config 恢复进设置，
/// 或用户选择只删不恢复——恢复值从 previous 参数进来）；hooks 里删掉 bridge 的条目，
/// 删空的事件键一并移除。
pub fn claude_unintegrate(settings: &Value, restore_statusline: Option<&str>) -> Result<MergeOutcome, String> {
    let mut root = settings.as_object().cloned().ok_or("settings.json 的顶层不是对象")?;
    let mut changes = Vec::new();

    if let Some(existing) = root.get("statusLine") {
        let cur = existing.get("command").and_then(|c| c.as_str()).unwrap_or("");
        if is_bridge_command(cur, "statusline") {
            match restore_statusline {
                Some(prev) if !prev.is_empty() => {
                    root.insert("statusLine".into(), json!({ "type": "command", "command": prev }));
                    changes.push(format!("statusLine 恢复为原来的命令「{prev}」"));
                }
                _ => {
                    root.remove("statusLine");
                    changes.push("删除 statusLine（mooncli-bridge）".into());
                }
            }
        }
    }
    if let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for event in HOOK_EVENTS {
            if let Some(list) = hooks.get_mut(event).and_then(|l| l.as_array_mut()) {
                let before = list.len();
                list.retain(|e| {
                    !e.get("hooks")
                        .and_then(|h| h.as_array())
                        .is_some_and(|hs| hs.iter().any(|h| h.get("command").and_then(|c| c.as_str()).is_some_and(|c| is_bridge_command(c, "hook"))))
                });
                if list.len() != before {
                    changes.push(format!("hooks.{event} 移除 mooncli-bridge hook"));
                }
            }
        }
        // 清掉变空的事件键和空 hooks 对象
        let empty_events: Vec<String> = hooks
            .iter()
            .filter(|(_, v)| v.as_array().is_some_and(|a| a.is_empty()))
            .map(|(k, _)| k.clone())
            .collect();
        for k in empty_events {
            hooks.remove(&k);
        }
    }
    if root.get("hooks").and_then(|h| h.as_object()).is_some_and(|o| o.is_empty()) {
        root.remove("hooks");
    }

    Ok(MergeOutcome { settings: Value::Object(root), changes, previous_statusline: None })
}

/// 当前是否已接入（statusLine 指向 bridge）。
pub fn claude_is_integrated(settings: &Value) -> bool {
    settings
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(|c| c.as_str())
        .is_some_and(|c| is_bridge_command(c, "statusline"))
}

// ---------- Codex config.toml ----------

/// 解析一行顶层 `notify = ["a", "b"]` 的字符串数组（只处理简单字符串元素，够用且不猜）。
fn parse_notify_array(text: &str) -> Option<Vec<String>> {
    let inner = text.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    let mut rest = inner;
    loop {
        rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let end = rest[1..].find(quote)? + 1;
        out.push(rest[1..end].to_string());
        rest = rest[end + 1..].trim_start();
        if rest.is_empty() {
            return Some(out);
        }
        rest = rest.strip_prefix(',')?;
    }
}

fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 在 TOML 文本里找顶层（第一个 [section] 之前）的 notify 行。返回 (行号, 解析出的数组)。
fn find_top_level_notify(lines: &[&str]) -> Option<(usize, Vec<String>)> {
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with('[') {
            break; // 进了 section，顶层结束
        }
        if let Some(rest) = t.strip_prefix("notify") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                if let Some(arr) = parse_notify_array(value.trim()) {
                    return Some((i, arr));
                }
            }
        }
    }
    None
}

pub struct TomlOutcome {
    pub text: String,
    pub changes: Vec<String>,
    /// 用户原来的 notify（存进 bridge-config 由 bridge 转调）
    pub previous_notify: Option<Vec<String>>,
}

/// 把 Codex 的 notify 接入 bridge：已有 notify 先保存再替换，没有就在文件顶部新增。
pub fn codex_integrate(toml_text: &str, bridge_path: &str) -> TomlOutcome {
    let want = vec![bridge_path.to_string(), "codex-notify".to_string()];
    let want_line = format!("notify = [{}]", want.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "));
    let lines: Vec<&str> = toml_text.lines().collect();
    let mut changes = Vec::new();
    if let Some((idx, existing)) = find_top_level_notify(&lines) {
        if existing.iter().any(|s| s.contains("mooncli-bridge")) {
            return TomlOutcome { text: toml_text.to_string(), changes: vec!["notify 已是 mooncli-bridge，不变".into()], previous_notify: None };
        }
        let previous = if existing.is_empty() { None } else { Some(existing) };
        if previous.is_some() {
            changes.push("原 notify 将保存并由 bridge 转调".into());
        }
        let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        out[idx] = want_line;
        changes.push("notify 改为 mooncli-bridge codex-notify".into());
        let mut text = out.join("\n");
        if toml_text.ends_with('\n') {
            text.push('\n');
        }
        TomlOutcome { text, changes, previous_notify: previous }
    } else {
        changes.push("新增 notify：mooncli-bridge codex-notify".into());
        TomlOutcome { text: format!("{want_line}\n{toml_text}"), changes, previous_notify: None }
    }
}

/// 撤销 Codex 接入：notify 是 bridge 时，恢复原 notify 或删掉这一行。
pub fn codex_unintegrate(toml_text: &str, restore: Option<&[String]>) -> TomlOutcome {
    let lines: Vec<&str> = toml_text.lines().collect();
    let mut changes = Vec::new();
    if let Some((idx, existing)) = find_top_level_notify(&lines) {
        if existing.iter().any(|s| s.contains("mooncli-bridge")) {
            let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            match restore {
                Some(prev) if !prev.is_empty() => {
                    out[idx] = format!("notify = [{}]", prev.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "));
                    changes.push("notify 恢复为原来的命令".into());
                }
                _ => {
                    out.remove(idx);
                    changes.push("删除 notify（mooncli-bridge）".into());
                }
            }
            let mut text = out.join("\n");
            if toml_text.ends_with('\n') && !text.is_empty() {
                text.push('\n');
            }
            return TomlOutcome { text, changes, previous_notify: None };
        }
    }
    TomlOutcome { text: toml_text.to_string(), changes, previous_notify: None }
}

/// 当前是否已接入（顶层 notify 指向 bridge）。
pub fn codex_is_integrated(toml_text: &str) -> bool {
    let lines: Vec<&str> = toml_text.lines().collect();
    find_top_level_notify(&lines).is_some_and(|(_, arr)| arr.iter().any(|s| s.contains("mooncli-bridge")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIDGE: &str = "C:\\Program Files\\mooncli\\mooncli-bridge.exe";

    #[test]
    fn claude_integrate_preserves_user_statusline_and_hooks() {
        let settings = json!({
            "statusLine": { "type": "command", "command": "my-status.sh" },
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "echo done" }] }] },
            "other": 1
        });
        let out = claude_integrate(&settings, BRIDGE).unwrap();
        assert_eq!(out.previous_statusline.as_deref(), Some("my-status.sh"));
        assert!(claude_is_integrated(&out.settings));
        let stop = out.settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2); // 用户的 + 追加的
        assert_eq!(out.settings["other"], 1);
        // 幂等：再接一次不重复追加
        let again = claude_integrate(&out.settings, BRIDGE).unwrap();
        assert_eq!(again.settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(again.previous_statusline.is_none());
    }

    #[test]
    fn claude_unintegrate_restores_and_cleans() {
        let settings = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "echo done" }] }] } });
        let integrated = claude_integrate(&settings, BRIDGE).unwrap().settings;
        let out = claude_unintegrate(&integrated, Some("my-status.sh")).unwrap();
        assert_eq!(out.settings["statusLine"]["command"], "my-status.sh");
        let stop = out.settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        // 全新设置接入后撤销，hooks 键应被清掉
        let fresh = claude_integrate(&json!({}), BRIDGE).unwrap().settings;
        let un = claude_unintegrate(&fresh, None).unwrap();
        assert!(un.settings.get("hooks").is_none());
        assert!(un.settings.get("statusLine").is_none());
    }

    #[test]
    fn codex_integrate_replaces_and_restores() {
        let toml = "model = \"gpt-5-codex\"\nnotify = [\"old-notify\", \"--flag\"]\n\n[profiles.x]\nnotify = [\"inner\"]\n";
        let out = codex_integrate(toml, BRIDGE);
        assert_eq!(out.previous_notify.as_deref(), Some(&["old-notify".to_string(), "--flag".to_string()][..]));
        assert!(codex_is_integrated(&out.text));
        assert!(out.text.contains("[profiles.x]")); // section 里的 notify 没被动
        let back = codex_unintegrate(&out.text, out.previous_notify.as_deref());
        assert!(back.text.contains("notify = [\"old-notify\", \"--flag\"]"));
        assert!(!codex_is_integrated(&back.text));
    }

    #[test]
    fn codex_integrate_inserts_at_top_when_absent() {
        let toml = "model = \"gpt-5-codex\"\n";
        let out = codex_integrate(toml, BRIDGE);
        assert!(out.text.starts_with("notify = "));
        assert!(out.previous_notify.is_none());
        let back = codex_unintegrate(&out.text, None);
        assert_eq!(back.text, toml);
    }
}
