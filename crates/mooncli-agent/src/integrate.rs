//! 一键接入的纯逻辑：把 mooncli-bridge 写进 Claude Code 的 settings.json（statusLine + hooks）
//! 和 Codex 的 config.toml（notify），以及精确撤销。只做数据变换不碰文件——备份、落盘、
//! 确认都在主进程和界面侧。
//!
//! 原则（docs/07 第 4 节）：
//! - 用户原来的 statusLine 整个对象保存进 bridge-config，bridge 运行时转调原命令，显示效果不变；
//! - hooks 是并列追加，不覆盖用户已有的同事件 hooks；撤销时只删 mooncli 加的那几条，
//!   只删 mooncli 自己新建的事件键；
//! - Codex 只改用户级 config.toml 顶层的 notify（项目级配置里的 notify 会被 Codex 忽略），
//!   原 notify 保存进 bridge-config 由 bridge 转调；撤销时恢复原样，其它内容逐字节不动。

use serde_json::{Map, Value, json};

// ---------- Claude Code settings.json ----------

/// bridge 在配置里写下的命令：`"<bridge 路径>" <子命令>`。路径用正斜杠——Claude Code 在 Windows 上
/// 用 Git Bash 执行命令，反斜杠在 bash 里会被当成转义；正斜杠在 bash 和 cmd 里都认。
pub fn bridge_command(bridge_path: &str, sub: &str) -> String {
    format!("\"{}\" {sub}", bridge_path.replace('\\', "/"))
}

fn is_bridge_command(cmd: &str, sub: &str) -> bool {
    cmd.contains("mooncli-bridge") && cmd.trim_end().ends_with(sub)
}

/// 要接入的 hook 事件：只要三个——提交提示词（记下会话属于哪个标签页、转录在哪）、
/// 需要你确认（Notification）、一轮结束（Stop）。后两个发完成提醒。
/// 工具调用前后的事件不接：触发太频繁，提醒和上下文都用不上。
pub const HOOK_EVENTS: [&str; 3] = ["UserPromptSubmit", "Notification", "Stop"];

fn entry_is_bridge(e: &Value) -> bool {
    e.get("hooks").and_then(Value::as_array).is_some_and(|hs| {
        hs.iter().any(|h| h.get("command").and_then(Value::as_str).is_some_and(|c| is_bridge_command(c, "claude-hook")))
    })
}

pub struct ClaudeOutcome {
    pub settings: Value,
    /// 给界面预览/提示用的人话变更清单
    pub changes: Vec<String>,
    /// 用户原来的 statusLine 对象（存进 bridge-config 让 bridge 转调、断开时恢复）
    pub previous_statusline: Option<Value>,
    /// 本次新建的 hooks 事件键（断开时只删这些键）
    pub created_events: Vec<String>,
    /// hooks 对象本身是不是本次新建的
    pub created_hooks: bool,
}

/// 把 statusLine + hooks 接入一份 Claude Code 设置。settings 必须是 JSON 对象（空文件按 null 传进来）。
pub fn claude_integrate(settings: &Value, bridge_path: &str) -> Result<ClaudeOutcome, String> {
    let mut root = match settings {
        Value::Null => Map::new(),
        v => v.as_object().cloned().ok_or("settings.json 的顶层不是对象")?,
    };
    let mut changes = Vec::new();
    let mut previous_statusline = None;

    let want_cmd = bridge_command(bridge_path, "claude-statusline");
    match root.get("statusLine") {
        Some(existing) => {
            let cur = existing.get("command").and_then(Value::as_str).unwrap_or("");
            if is_bridge_command(cur, "claude-statusline") {
                changes.push("statusLine 已经是 mooncli-bridge，不变".into());
            } else {
                if !cur.is_empty() {
                    changes.push(format!("原 statusLine「{cur}」会保存下来，由 bridge 转调，显示效果不变"));
                }
                previous_statusline = Some(existing.clone());
                // 保留用户的 padding、refreshInterval 等设置，只换命令
                let mut obj = existing.as_object().cloned().unwrap_or_default();
                obj.insert("type".into(), json!("command"));
                obj.insert("command".into(), json!(want_cmd));
                root.insert("statusLine".into(), Value::Object(obj));
                changes.push("statusLine 改为 mooncli-bridge claude-statusline".into());
            }
        }
        None => {
            root.insert("statusLine".into(), json!({ "type": "command", "command": want_cmd }));
            changes.push("新增 statusLine：mooncli-bridge claude-statusline（状态栏显示模型、上下文与额度）".into());
        }
    }

    let hook_cmd = bridge_command(bridge_path, "claude-hook");
    let created_hooks = !root.contains_key("hooks");
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks_obj = hooks.as_object_mut().ok_or("settings.json 的 hooks 不是对象")?;
    let mut created_events = Vec::new();
    let mut appended = Vec::new();
    for event in HOOK_EVENTS {
        let entry = json!({ "hooks": [{ "type": "command", "command": hook_cmd }] });
        match hooks_obj.get_mut(event) {
            Some(list) => {
                let arr = list.as_array_mut().ok_or(format!("hooks.{event} 不是数组"))?;
                if !arr.iter().any(entry_is_bridge) {
                    arr.push(entry);
                    appended.push(event);
                }
            }
            None => {
                hooks_obj.insert(event.into(), json!([entry]));
                created_events.push(event.to_string());
                appended.push(event);
            }
        }
    }
    // 早先的版本接过更多事件，顺手把那些多余的 bridge 条目清掉
    let mut emptied = Vec::new();
    for (event, list) in hooks_obj.iter_mut() {
        if HOOK_EVENTS.contains(&event.as_str()) {
            continue;
        }
        if let Some(arr) = list.as_array_mut() {
            let before = arr.len();
            arr.retain(|e| !entry_is_bridge(e));
            if arr.is_empty() && before > 0 {
                emptied.push(event.clone());
            }
        }
    }
    for event in emptied {
        hooks_obj.remove(&event);
    }
    if appended.is_empty() {
        changes.push("hooks 已经接入，不变".into());
    } else {
        changes.push(format!("hooks 追加 mooncli-bridge（{}），你原有的 hooks 不动", appended.join("、")));
    }

    Ok(ClaudeOutcome { settings: Value::Object(root), changes, previous_statusline, created_events, created_hooks })
}

/// 撤销接入：statusLine 恢复原对象（没有就删掉），hooks 里删掉 bridge 的条目；
/// 只删接入时新建的事件键和 hooks 对象，用户自己的空数组之类原样保留。
pub fn claude_unintegrate(
    settings: &Value,
    restore_statusline: Option<&Value>,
    created_events: &[String],
    created_hooks: bool,
) -> Result<ClaudeOutcome, String> {
    let mut root = settings.as_object().cloned().ok_or("settings.json 的顶层不是对象")?;
    let mut changes = Vec::new();

    if let Some(existing) = root.get("statusLine") {
        let cur = existing.get("command").and_then(Value::as_str).unwrap_or("");
        if is_bridge_command(cur, "claude-statusline") {
            match restore_statusline {
                Some(prev) => {
                    root.insert("statusLine".into(), prev.clone());
                    changes.push("statusLine 恢复为原来的设置".into());
                }
                None => {
                    root.remove("statusLine");
                    changes.push("删除 statusLine（mooncli-bridge）".into());
                }
            }
        }
    }
    let mut removed = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        // 所有事件都看一遍：早先的版本接过更多事件
        let events: Vec<String> = hooks.keys().cloned().collect();
        for event in events {
            if let Some(list) = hooks.get_mut(&event).and_then(Value::as_array_mut) {
                let before = list.len();
                list.retain(|e| !entry_is_bridge(e));
                removed |= list.len() != before;
                if list.is_empty() && created_events.contains(&event) {
                    hooks.remove(&event);
                }
            }
        }
    }
    if removed {
        changes.push("hooks 里删掉 mooncli-bridge 的条目".into());
    }
    if created_hooks && root.get("hooks").and_then(Value::as_object).is_some_and(|o| o.is_empty()) {
        root.remove("hooks");
    }

    Ok(ClaudeOutcome {
        settings: Value::Object(root),
        changes,
        previous_statusline: None,
        created_events: Vec::new(),
        created_hooks: false,
    })
}

/// 当前是否已接入（statusLine 指向 bridge）。
pub fn claude_is_integrated(settings: &Value) -> bool {
    settings
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
        .is_some_and(|c| is_bridge_command(c, "claude-statusline"))
}

// ---------- Codex config.toml ----------

/// 解析 TOML 基本字符串（双引号）里的转义。只认常见的几种，认不出就当解析失败，不猜。
fn unescape_basic(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// 解析一行顶层 `notify = ["a", 'b']` 的字符串数组。双引号是基本字符串（处理转义），
/// 单引号是字面量字符串（原样）。别的写法（多行数组、行尾注释以外的东西）不认，返回 None。
fn parse_notify_array(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    let text = match text.rfind(']') {
        // 允许行尾注释：`notify = ["x"] # 说明`
        Some(i) if text[i + 1..].trim().is_empty() || text[i + 1..].trim_start().starts_with('#') => &text[..=i],
        _ => return None,
    };
    let inner = text.strip_prefix('[')?.strip_suffix(']')?.trim();
    let mut out = Vec::new();
    let mut rest = inner;
    while !rest.is_empty() {
        let quote = rest.chars().next()?;
        let body = &rest[1..];
        let end = match quote {
            '\'' => body.find('\'')?,
            '"' => {
                // 找到不被反斜杠转义的那个双引号
                let mut escaped = false;
                body.char_indices()
                    .find(|&(_, c)| {
                        let hit = c == '"' && !escaped;
                        escaped = c == '\\' && !escaped;
                        hit
                    })?
                    .0
            }
            _ => return None,
        };
        let raw = &body[..end];
        out.push(if quote == '"' { unescape_basic(raw)? } else { raw.to_string() });
        rest = body[end + 1..].trim_start();
        if let Some(r) = rest.strip_prefix(',') {
            rest = r.trim_start();
        } else if !rest.is_empty() {
            return None;
        }
    }
    Some(out)
}

fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 按行切分且保留每行的换行符（CRLF 的文件改完还是 CRLF）。返回 (内容, 换行符)。
fn split_lines(text: &str) -> Vec<(&str, &str)> {
    text.split_inclusive('\n')
        .map(|seg| {
            if let Some(c) = seg.strip_suffix("\r\n") {
                (c, "\r\n")
            } else if let Some(c) = seg.strip_suffix('\n') {
                (c, "\n")
            } else {
                (seg, "")
            }
        })
        .collect()
}

/// 在 TOML 文本里找顶层（第一个 [section] 之前）的 notify 行。返回 (行号, 解析出的数组)。
fn find_top_level_notify(lines: &[(&str, &str)]) -> Option<(usize, Vec<String>)> {
    for (i, (line, _)) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with('[') {
            break; // 进了 section，顶层结束
        }
        if let Some(rest) = t.strip_prefix("notify") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=')
                && let Some(arr) = parse_notify_array(value)
            {
                return Some((i, arr));
            }
        }
    }
    None
}

fn join_lines(lines: &[(String, &str)]) -> String {
    lines.iter().map(|(c, nl)| format!("{c}{nl}")).collect()
}

pub struct TomlOutcome {
    pub text: String,
    pub changes: Vec<String>,
    /// 用户原来的 notify（存进 bridge-config 由 bridge 转调）
    pub previous_notify: Option<Vec<String>>,
    /// 用户原来那一行的原文（断开时逐字节放回去）
    pub previous_line: Option<String>,
}

/// 把 Codex 的 notify 接入 bridge：已有 notify 先保存再替换，没有就在文件顶部新增。
pub fn codex_integrate(toml_text: &str, bridge_path: &str) -> TomlOutcome {
    let want = [bridge_path.to_string(), "codex-notify".to_string()];
    let want_line = format!("notify = [{}]", want.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "));
    let lines = split_lines(toml_text);
    let mut changes = Vec::new();
    if let Some((idx, existing)) = find_top_level_notify(&lines) {
        if existing.iter().any(|s| s.contains("mooncli-bridge")) {
            return TomlOutcome {
                text: toml_text.to_string(),
                changes: vec!["notify 已是 mooncli-bridge，不变".into()],
                previous_notify: None,
                previous_line: None,
            };
        }
        let previous = if existing.is_empty() { None } else { Some(existing) };
        if previous.is_some() {
            changes.push("原 notify 会保存下来，由 bridge 转调，行为不变".into());
        }
        let previous_line = Some(lines[idx].0.to_string());
        let mut out: Vec<(String, &str)> = lines.iter().map(|(c, nl)| (c.to_string(), *nl)).collect();
        out[idx].0 = want_line;
        changes.push("notify 改为 mooncli-bridge codex-notify".into());
        TomlOutcome { text: join_lines(&out), changes, previous_notify: previous, previous_line }
    } else {
        changes.push("新增 notify：mooncli-bridge codex-notify".into());
        let nl = if toml_text.contains("\r\n") { "\r\n" } else { "\n" };
        TomlOutcome { text: format!("{want_line}{nl}{toml_text}"), changes, previous_notify: None, previous_line: None }
    }
}

/// 撤销 Codex 接入：notify 是 bridge 时，放回原来那一行（没有原文时按保存的数组重写），
/// 原来没有 notify 就删掉这一行。其它内容逐字节不动。
pub fn codex_unintegrate(toml_text: &str, restore_line: Option<&str>, restore: Option<&[String]>) -> TomlOutcome {
    let lines = split_lines(toml_text);
    let mut changes = Vec::new();
    if let Some((idx, existing)) = find_top_level_notify(&lines)
        && existing.iter().any(|s| s.contains("mooncli-bridge"))
    {
        let mut out: Vec<(String, &str)> = lines.iter().map(|(c, nl)| (c.to_string(), *nl)).collect();
        match (restore_line, restore) {
            (Some(line), _) => {
                out[idx].0 = line.to_string();
                changes.push("notify 恢复为原来的那一行".into());
            }
            (None, Some(prev)) if !prev.is_empty() => {
                out[idx].0 =
                    format!("notify = [{}]", prev.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "));
                changes.push("notify 恢复为原来的命令".into());
            }
            _ => {
                out.remove(idx);
                changes.push("删除 notify（mooncli-bridge）".into());
            }
        }
        return TomlOutcome { text: join_lines(&out), changes, previous_notify: None, previous_line: None };
    }
    TomlOutcome { text: toml_text.to_string(), changes, previous_notify: None, previous_line: None }
}

/// 当前是否已接入（顶层 notify 指向 bridge）。
pub fn codex_is_integrated(toml_text: &str) -> bool {
    find_top_level_notify(&split_lines(toml_text))
        .is_some_and(|(_, arr)| arr.iter().any(|s| s.contains("mooncli-bridge")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_command_uses_forward_slashes() {
        assert_eq!(
            bridge_command(BRIDGE, "claude-hook"),
            "\"C:/Program Files/mooncli/mooncli-bridge.exe\" claude-hook"
        );
    }

    #[test]
    fn claude_integrate_preserves_user_statusline_and_hooks() {
        let settings = json!({
            "statusLine": { "type": "command", "command": "bash ~/.claude/status.sh", "padding": 1 },
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "echo done" }] }], "Notification": [] },
            "other": 1
        });
        let out = claude_integrate(&settings, BRIDGE).unwrap();
        assert_eq!(out.previous_statusline.as_ref().unwrap()["command"], "bash ~/.claude/status.sh");
        assert!(claude_is_integrated(&out.settings));
        assert_eq!(out.settings["statusLine"]["padding"], 1);
        assert_eq!(out.settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(out.settings["other"], 1);
        assert!(!out.created_hooks);
        assert!(!out.created_events.iter().any(|e| e == "Stop" || e == "Notification"));
        // 幂等：再接一次不重复追加
        let again = claude_integrate(&out.settings, BRIDGE).unwrap();
        assert_eq!(again.settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        // 撤销后与原来完全一致（包括用户自己的空数组和键的顺序）
        let back =
            claude_unintegrate(&out.settings, out.previous_statusline.as_ref(), &out.created_events, out.created_hooks)
                .unwrap();
        assert_eq!(serde_json::to_string(&back.settings).unwrap(), serde_json::to_string(&settings).unwrap());
    }

    #[test]
    fn claude_fresh_settings_round_trip() {
        let fresh = claude_integrate(&Value::Null, BRIDGE).unwrap();
        assert!(fresh.created_hooks);
        let un = claude_unintegrate(&fresh.settings, None, &fresh.created_events, fresh.created_hooks).unwrap();
        assert_eq!(un.settings, json!({}));
    }

    const BRIDGE: &str = "C:\\Program Files\\mooncli\\mooncli-bridge.exe";

    #[test]
    fn codex_integrate_replaces_and_restores() {
        let toml =
            "model = \"gpt-5-codex\"\nnotify = [\"old-notify\", \"--flag\"]\n\n[profiles.x]\nnotify = [\"inner\"]\n";
        let out = codex_integrate(toml, BRIDGE);
        assert_eq!(out.previous_notify.as_deref(), Some(&["old-notify".to_string(), "--flag".to_string()][..]));
        assert!(codex_is_integrated(&out.text));
        assert!(out.text.contains("[profiles.x]")); // section 里的 notify 没被动
        let back = codex_unintegrate(&out.text, out.previous_line.as_deref(), out.previous_notify.as_deref());
        assert_eq!(back.text, toml);
        assert!(!codex_is_integrated(&back.text));
    }

    #[test]
    fn codex_integrate_inserts_at_top_when_absent() {
        let toml = "model = \"gpt-5-codex\"\n";
        let out = codex_integrate(toml, BRIDGE);
        assert!(out.text.starts_with("notify = "));
        assert!(out.previous_notify.is_none());
        let back = codex_unintegrate(&out.text, None, None);
        assert_eq!(back.text, toml);
    }

    #[test]
    fn codex_literal_strings_and_crlf_round_trip_byte_exact() {
        // 真实遇到的写法：单引号字面量字符串带反斜杠，第三个元素里还嵌着一段 JSON；文件是 CRLF
        let line = r#"notify = ['C:\Users\me\codex-computer-use.exe', "turn-ended", "--previous-notify", '["C:\\Users\\me\\old.exe","turn-ended"]']"#;
        let toml = format!("model = \"gpt-5\"\r\n{line}\r\n\r\n[features]\r\nx = true\r\n");
        let out = codex_integrate(&toml, BRIDGE);
        let prev = out.previous_notify.clone().unwrap();
        assert_eq!(prev[0], r"C:\Users\me\codex-computer-use.exe");
        assert_eq!(prev[3], r#"["C:\\Users\\me\\old.exe","turn-ended"]"#);
        assert!(out.text.contains("\r\n[features]\r\n"));
        let back = codex_unintegrate(&out.text, out.previous_line.as_deref(), Some(&prev));
        assert_eq!(back.text, toml);
    }

    #[test]
    fn codex_basic_string_escapes() {
        assert_eq!(
            parse_notify_array(r#"["C:\\Tools\\n.exe", "a\"b"] # 注释"#).unwrap(),
            vec![r"C:\Tools\n.exe".to_string(), "a\"b".to_string()]
        );
        assert!(parse_notify_array(r#"["x" "y"]"#).is_none());
    }
}
