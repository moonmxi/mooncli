//! 程序设置的标题怎么用。规则见 docs/06-smart-tabs.md 第 3 节。

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CleanTitle {
    /// 拆出来的状态符号（转圈的点、✳ 等），标题文字保持稳定，不让标签宽度来回抖。
    pub activity: Option<String>,
    pub text: String,
}

/// 把标题开头的状态符号和文字分开。
pub fn clean_title(raw: &str) -> CleanTitle {
    let trimmed = raw.trim();
    let mut split = 0;
    for (i, c) in trimmed.char_indices() {
        // 状态符号之间、之后的空白也算进状态部分
        if !(is_activity_char(c) || (c.is_whitespace() && split > 0)) {
            break;
        }
        split = i + c.len_utf8();
    }
    let activity = trimmed[..split].trim();
    CleanTitle {
        activity: (!activity.is_empty()).then(|| activity.to_string()),
        text: trimmed[split..].trim().to_string(),
    }
}

fn is_activity_char(c: char) -> bool {
    matches!(c,
        '\u{2800}'..='\u{28FF}'          // 盲文点阵，常用作转圈动画
        | '\u{2733}' | '\u{2734}' | '\u{2736}' | '\u{273B}' | '\u{273D}' | '\u{2722}' // ✳ ✴ ✶ ✻ ✽ ✢
        | '\u{25CF}' | '\u{25CB}' | '\u{25D0}'..='\u{25D3}' // ● ○ ◐ ◑ ◒ ◓
        | '\u{00B7}' | '\u{2022}' | '*'  // · • *
        | '\u{231B}' | '\u{23F3}'        // ⌛ ⏳
    )
}

/// pwsh 自己设的标题（`pwsh`、可执行文件路径、"管理员: …" 前缀）没信息量，应该忽略，退回自动生成。
pub fn is_uninformative(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let t = t.strip_prefix("Administrator: ").or_else(|| t.strip_prefix("管理员: ")).unwrap_or(t);
    let lower = t.to_ascii_lowercase().replace('\\', "/");
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    matches!(
        file,
        "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe" | "windows powershell" | "powershell 7" | "cmd.exe"
    ) || lower.starts_with("powershell 7")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_leading_activity_symbols() {
        assert_eq!(
            clean_title("✳ 重构标签页布局"),
            CleanTitle { activity: Some("✳".into()), text: "重构标签页布局".into() }
        );
        assert_eq!(
            clean_title("⠋ Fixing tests"),
            CleanTitle { activity: Some("⠋".into()), text: "Fixing tests".into() }
        );
        assert_eq!(clean_title("plain title"), CleanTitle { activity: None, text: "plain title".into() });
    }

    #[test]
    fn symbol_inside_text_is_kept() {
        assert_eq!(clean_title("a * b").text, "a * b");
    }

    #[test]
    fn pwsh_titles_are_uninformative() {
        for t in ["pwsh", "C:\\Program Files\\PowerShell\\7\\pwsh.exe", "Administrator: pwsh", "PowerShell 7.5.0", ""] {
            assert!(is_uninformative(t), "{t}");
        }
        assert!(!is_uninformative("Claude Code"));
        assert!(!is_uninformative("vim main.rs"));
    }
}
