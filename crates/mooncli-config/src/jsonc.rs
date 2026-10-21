//! 去掉 JSON 里的注释和多余的逗号，让设置文件可以像 VS Code 那样写注释。
//! 被去掉的字符换成空格（换行保留），这样 serde_json 报错的行列号仍然对得上原文件。

pub fn strip(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out.push(b' ');
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                out.extend_from_slice(b"  ");
                i += 2;
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    out.push(if b[i] == b'\n' { b'\n' } else { b' ' });
                    i += 1;
                }
                if i < b.len() {
                    out.extend_from_slice(b"  ");
                    i += 2;
                }
            }
            b',' => {
                // 后面（跳过空白和注释后）紧跟 } 或 ] 的逗号是多余的
                let mut j = i + 1;
                loop {
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if b.get(j) == Some(&b'/') && b.get(j + 1) == Some(&b'/') {
                        while j < b.len() && b[j] != b'\n' {
                            j += 1;
                        }
                    } else if b.get(j) == Some(&b'/') && b.get(j + 1) == Some(&b'*') {
                        j += 2;
                        while j < b.len() && !(b[j] == b'*' && b.get(j + 1) == Some(&b'/')) {
                            j += 1;
                        }
                        j += 2;
                    } else {
                        break;
                    }
                }
                out.push(if matches!(b.get(j), Some(b'}') | Some(b']')) { b' ' } else { b',' });
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    // 只替换了 ASCII 字节，UTF-8 结构不变
    String::from_utf8(out).expect("UTF-8 保持不变")
}

/// 解析带注释的 JSON，出错时带上文件名和行列号。
pub fn parse<T: serde::de::DeserializeOwned>(file: &str, src: &str) -> crate::Result<T> {
    serde_json::from_str(&strip(src)).map_err(|e| crate::Error::Parse {
        file: file.to_string(),
        line: e.line(),
        column: e.column(),
        msg: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_trailing_commas() {
        let src = "{\n  // 注释\n  \"a\": \"http://x\", /* 块\n注释 */\n  \"b\": [1, 2,],\n}";
        let v: serde_json::Value = serde_json::from_str(&strip(src)).unwrap();
        assert_eq!(v, serde_json::json!({"a": "http://x", "b": [1, 2]}));
    }

    #[test]
    fn strings_with_comment_like_text_untouched() {
        let src = r#"{"p": "C:\\a//b", "q": "/* no */", "r": "\"//\""}"#;
        let v: serde_json::Value = serde_json::from_str(&strip(src)).unwrap();
        assert_eq!(v["p"], "C:\\a//b");
        assert_eq!(v["q"], "/* no */");
        assert_eq!(v["r"], "\"//\"");
    }

    #[test]
    fn error_line_matches_original() {
        let src = "{\n  // 第二行是注释\n  \"a\": 1\n  \"b\": 2\n}";
        let e = parse::<serde_json::Value>("settings.json", src).unwrap_err();
        let crate::Error::Parse { line, .. } = e else { panic!() };
        assert_eq!(line, 4);
    }
}
