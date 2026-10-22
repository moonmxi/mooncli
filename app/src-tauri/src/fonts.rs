//! 列出系统里装的字体（docs/03 §2 的字体选择器）。界面再从里面挑出等宽的。
//! 读注册表的字体表：所有用户的在 HKLM，只给当前用户装的在 HKCU。

/// 字体表里的一个名字 → 字体家族名：去掉 "(TrueType)"、字重和样式后缀；
/// "Cascadia Code & Cascadia Mono" 这样一个文件装多个家族的拆开。
pub fn family_names(entry: &str) -> Vec<String> {
    let name = entry.trim();
    let name = match name.rfind(" (") {
        Some(i) if name.ends_with(')') => &name[..i],
        _ => name,
    };
    const STYLES: [&str; 22] = [
        "Regular",
        "Bold",
        "Italic",
        "Oblique",
        "Light",
        "SemiLight",
        "Semilight",
        "ExtraLight",
        "Extralight",
        "UltraLight",
        "Medium",
        "SemiBold",
        "Semibold",
        "DemiBold",
        "ExtraBold",
        "Heavy",
        "Black",
        "Thin",
        "Condensed",
        "Narrow",
        "Book",
        "Variable",
    ];
    name.split(" & ")
        .filter_map(|part| {
            let mut words: Vec<&str> = part.split_whitespace().collect();
            while words.len() > 1 && STYLES.contains(words.last().unwrap()) {
                words.pop();
            }
            let fam = words.join(" ");
            (!fam.is_empty()).then_some(fam)
        })
        .collect()
}

#[cfg(windows)]
fn registry_entries() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    let mut out = Vec::new();
    for key in [
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
        r"HKCU\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
    ] {
        let Ok(o) = std::process::Command::new("reg").args(["query", key]).creation_flags(0x0800_0000).output() else {
            continue;
        };
        // reg 的输出按系统代码页编码；字体名基本是英文，乱码的个别中文名不影响挑等宽字体
        let text = String::from_utf8_lossy(&o.stdout);
        for line in text.lines() {
            if let Some(i) = line.find("    REG_SZ") {
                out.push(line[..i].trim().to_string());
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn registry_entries() -> Vec<String> {
    Vec::new()
}

/// 全部字体家族名，去重、排好序
#[tauri::command]
pub fn system_fonts() -> Vec<String> {
    let mut names: Vec<String> = registry_entries().iter().flat_map(|e| family_names(e)).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

#[cfg(test)]
mod tests {
    use super::family_names;

    #[test]
    fn strips_styles_and_splits_collections() {
        assert_eq!(family_names("Cascadia Code Bold (TrueType)"), vec!["Cascadia Code"]);
        assert_eq!(family_names("Cascadia Code & Cascadia Mono (TrueType)"), vec!["Cascadia Code", "Cascadia Mono"]);
        assert_eq!(family_names("JetBrains Mono ExtraBold Italic (TrueType)"), vec!["JetBrains Mono"]);
        assert_eq!(family_names("Consolas"), vec!["Consolas"]);
        // 只有一个词时不能把家族名本身当成样式去掉
        assert_eq!(family_names("Black (TrueType)"), vec!["Black"]);
    }
}
