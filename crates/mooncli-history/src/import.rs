//! 解析 PSReadLine 的历史文件（`(Get-PSReadLineOption).HistorySavePath`）。
//!
//! PSReadLine 保存多行命令时，除最后一行外每行末尾加一个反引号。

pub fn parse_psreadline_history(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for line in content.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (body, continues) = match line.strip_suffix('`') {
            Some(b) => (b, true),
            None => (line, false),
        };
        match cur.as_mut() {
            Some(c) => {
                c.push('\n');
                c.push_str(body);
            }
            None => cur = Some(body.to_string()),
        }
        if !continues {
            let c = cur.take().unwrap();
            if !c.trim().is_empty() {
                out.push(c);
            }
        }
    }
    if let Some(c) = cur.filter(|c| !c.trim().is_empty()) {
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_and_multiline() {
        let content = "git status\r\nforeach ($i in 1..3) {`\r\n  $i`\r\n}\r\n\r\nls\r\n";
        assert_eq!(
            parse_psreadline_history(content),
            vec!["git status", "foreach ($i in 1..3) {\n  $i\n}", "ls"]
        );
    }

    #[test]
    fn dangling_continuation_at_eof() {
        assert_eq!(parse_psreadline_history("a`\nb`"), vec!["a\nb"]);
    }
}
