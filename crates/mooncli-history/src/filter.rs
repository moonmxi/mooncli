//! 哪些命令不进历史库。

use regex::Regex;

/// 默认规则：命令里像是带了密码、令牌、密钥的，都不记。
const DEFAULT_RULES: &[&str] = &[
    r"(?i)(password|passwd|token|secret|api[_-]?key|access[_-]?key)\s*[=:]",
    r"(?i)(^|\s)--?(password|passwd|token|secret|api[_-]?key)\b",
    r"\bsk-[A-Za-z0-9_-]{16,}",
    r"\bgh[pousr]_[A-Za-z0-9]{20,}",
    r"\bgithub_pat_[A-Za-z0-9_]{20,}",
    r"\bAKIA[0-9A-Z]{16}\b",
    r"\bxox[abpr]-[A-Za-z0-9-]{10,}",
    r"(?i)ConvertTo-SecureString\b.*-AsPlainText",
];

pub struct Filter {
    rules: Vec<Regex>,
    ignore_leading_space: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self::new(&[]).expect("内置规则必须能编译")
    }
}

impl Filter {
    /// `extra` 是用户在设置里加的规则（正则）。
    pub fn new(extra: &[String]) -> crate::Result<Self> {
        let mut rules = Vec::new();
        for r in DEFAULT_RULES.iter().copied().chain(extra.iter().map(String::as_str)) {
            rules.push(Regex::new(r)?);
        }
        Ok(Self { rules, ignore_leading_space: true })
    }

    pub fn should_skip(&self, text: &str) -> bool {
        if text.trim().is_empty() {
            return true;
        }
        if self.ignore_leading_space && text.starts_with(' ') {
            return true;
        }
        self.rules.iter().any(|r| r.is_match(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_secrets_and_leading_space() {
        let f = Filter::default();
        for t in [
            " git push",
            "mysql -u root --password hunter2",
            "$env:OPENAI_API_KEY=abc",
            "curl -H 'Authorization: Bearer ghp_abcdefghijklmnopqrstuvwxyz0123'",
            "set TOKEN=xyz",
            "ConvertTo-SecureString 'p' -AsPlainText -Force",
            "echo sk-ant-api03-abcdefghijklmnop",
            "",
        ] {
            assert!(f.should_skip(t), "应该跳过: {t:?}");
        }
    }

    #[test]
    fn keeps_normal_commands() {
        let f = Filter::default();
        for t in ["git status", "pwd", "Get-Secret -Name foo", "npm run build", "cd tokens/"] {
            assert!(!f.should_skip(t), "不该跳过: {t:?}");
        }
    }

    #[test]
    fn user_rules_are_added() {
        let f = Filter::new(&["^internal-tool ".into()]).unwrap();
        assert!(f.should_skip("internal-tool login"));
        assert!(Filter::new(&["(".into()]).is_err());
    }
}
