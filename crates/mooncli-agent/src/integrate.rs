//! 一键接入的纯逻辑：把 mooncli-bridge 写进 Codex 的 config.toml（notify），以及精确撤销。
//! 只做文本变换不碰文件——备份、落盘、确认都在主进程和界面侧。
//!
//! 原则（docs/07 第 4 节）：
//! - 只改用户级 config.toml 顶层的 notify（项目级配置里的 notify 会被 Codex 忽略）；
//! - 用户原来的 notify 保存进 bridge-config（previousCodexNotify），bridge 运行时转调；
//! - 撤销时恢复原 notify（没有就删掉这一行），其它内容逐字节不动。

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
            if let Some(value) = rest.strip_prefix('=')
                && let Some(arr) = parse_notify_array(value.trim())
            {
                return Some((i, arr));
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
    let want = [bridge_path.to_string(), "codex-notify".to_string()];
    let want_line = format!("notify = [{}]", want.iter().map(|s| toml_string(s)).collect::<Vec<_>>().join(", "));
    let lines: Vec<&str> = toml_text.lines().collect();
    let mut changes = Vec::new();
    if let Some((idx, existing)) = find_top_level_notify(&lines) {
        if existing.iter().any(|s| s.contains("mooncli-bridge")) {
            return TomlOutcome {
                text: toml_text.to_string(),
                changes: vec!["notify 已是 mooncli-bridge，不变".into()],
                previous_notify: None,
            };
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
    if let Some((idx, existing)) = find_top_level_notify(&lines)
        && existing.iter().any(|s| s.contains("mooncli-bridge"))
    {
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
    fn codex_integrate_replaces_and_restores() {
        let toml =
            "model = \"gpt-5-codex\"\nnotify = [\"old-notify\", \"--flag\"]\n\n[profiles.x]\nnotify = [\"inner\"]\n";
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
