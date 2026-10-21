//! 主题：加载、继承（extends）、组件色推导、对比度检查。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::color::{Rgba, contrast};
use crate::{Error, Result, jsonc};

/// 内置主题，源文件在 design/themes/，改了要跑 tools/check_contrast.py。
pub const BUILTIN: &[(&str, &str)] = &[
    ("moon-paper", include_str!("../../../design/themes/moon-paper.json")),
    ("frost", include_str!("../../../design/themes/frost.json")),
    ("sakura-dawn", include_str!("../../../design/themes/sakura-dawn.json")),
    ("celadon", include_str!("../../../design/themes/celadon.json")),
    ("night-voyage", include_str!("../../../design/themes/night-voyage.json")),
];

pub const DEFAULT_LIGHT: &str = "moon-paper";
pub const DEFAULT_DARK: &str = "night-voyage";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalColors {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub cursor_accent: String,
    pub selection_background: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl TerminalColors {
    pub fn ansi(&self) -> [(&'static str, &str); 16] {
        [
            ("black", &self.black),
            ("red", &self.red),
            ("green", &self.green),
            ("yellow", &self.yellow),
            ("blue", &self.blue),
            ("magenta", &self.magenta),
            ("cyan", &self.cyan),
            ("white", &self.white),
            ("brightBlack", &self.bright_black),
            ("brightRed", &self.bright_red),
            ("brightGreen", &self.bright_green),
            ("brightYellow", &self.bright_yellow),
            ("brightBlue", &self.bright_blue),
            ("brightMagenta", &self.bright_magenta),
            ("brightCyan", &self.bright_cyan),
            ("brightWhite", &self.bright_white),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiColors {
    pub surface0: String,
    pub surface1: String,
    pub surface2: String,
    pub border: String,
    pub text: String,
    pub text_muted: String,
    pub accent: String,
    pub accent_soft: String,
    pub on_accent: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub name: String,
    pub display_name: String,
    pub kind: Kind,
    #[serde(default)]
    pub description: String,
    pub terminal: TerminalColors,
    pub ui: UiColors,
    /// 单独改某个组件的颜色，键名见 [`components_table`]
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
}

/// 组件色和它的默认推导规则（docs/03 §1"两层颜色"），和界面共用 resources/theme-components.json。
/// 值是 `基础色名` 或 `基础色名/不透明度`。
pub fn components_table() -> &'static [(String, String)] {
    static TABLE: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        #[derive(Deserialize)]
        struct File {
            components: serde_json::Map<String, Value>,
        }
        let f: File = serde_json::from_str(include_str!("../../../resources/theme-components.json"))
            .expect("theme-components.json 格式不对");
        f.components.into_iter().map(|(k, v)| (k, v.as_str().expect("规则必须是字符串").to_string())).collect()
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContrastIssue {
    pub what: String,
    pub ratio: f64,
    pub need: f64,
}

impl Theme {
    fn base_color(&self, name: &str) -> Option<&str> {
        let u = &self.ui;
        Some(match name {
            "surface0" => &u.surface0,
            "surface1" => &u.surface1,
            "surface2" => &u.surface2,
            "border" => &u.border,
            "text" => &u.text,
            "textMuted" => &u.text_muted,
            "accent" => &u.accent,
            "accentSoft" => &u.accent_soft,
            "onAccent" => &u.on_accent,
            "success" => &u.success,
            "warning" => &u.warning,
            "danger" => &u.danger,
            _ => return None,
        })
    }

    /// 所有组件色的最终取值：`overrides` 优先，否则按推导规则。
    pub fn components(&self) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        for (key, rule) in components_table() {
            let value = match self.overrides.get(key) {
                Some(v) => {
                    Rgba::parse(v)?;
                    v.clone()
                }
                None => {
                    let (name, alpha) = match rule.split_once('/') {
                        Some((n, a)) => (n, a.parse::<f64>().ok()),
                        None => (rule.as_str(), None),
                    };
                    let base = self.base_color(name).expect("推导规则只引用基础色");
                    match alpha {
                        Some(a) => Rgba::parse(base)?.with_alpha(a).to_hex(),
                        None => base.to_string(),
                    }
                }
            };
            out.insert(key.to_string(), value);
        }
        Ok(out)
    }

    /// 对比度检查，规则和 tools/check_contrast.py 一致。返回不达标的项。
    pub fn check_contrast(&self) -> Result<Vec<ContrastIssue>> {
        let p = Rgba::parse;
        let t = &self.terminal;
        let bg = p(&t.background)?;
        let mut issues = Vec::new();
        let mut want = |what: String, a: &str, b: Rgba, need: f64| -> Result<()> {
            let r = contrast(p(a)?, b);
            if r < need {
                issues.push(ContrastIssue { what, ratio: (r * 100.0).round() / 100.0, need });
            }
            Ok(())
        };
        want("foreground".into(), &t.foreground, bg, 7.0)?;
        want("selection".into(), &t.foreground, p(&t.selection_background)?, 4.5)?;
        want("cursor".into(), &t.cursor, bg, 3.0)?;
        let soft: &[&str] = match self.kind {
            Kind::Light => &["white", "brightWhite", "brightBlack"],
            Kind::Dark => &["black", "brightBlack"],
        };
        for (name, c) in t.ansi() {
            want(name.into(), c, bg, if soft.contains(&name) { 3.0 } else { 4.5 })?;
        }
        let u = &self.ui;
        for (sname, s) in [("surface0", &u.surface0), ("surface1", &u.surface1), ("surface2", &u.surface2)] {
            let s = p(s)?;
            want(format!("ui.text/{sname}"), &u.text, s, 7.0)?;
            want(format!("ui.textMuted/{sname}"), &u.text_muted, s, 4.5)?;
        }
        want("ui.accent/surface1".into(), &u.accent, p(&u.surface1)?, 3.0)?;
        want("ui.onAccent/accent".into(), &u.on_accent, p(&u.accent)?, 4.5)?;
        Ok(issues)
    }
}

/// 一组可用的主题：内置 + 用户目录里的。
#[derive(Debug, Clone, Default)]
pub struct ThemeSet {
    themes: BTreeMap<String, Theme>,
    /// 加载失败的用户主题和原因，设置界面里展示
    pub errors: Vec<String>,
}

impl ThemeSet {
    pub fn builtin() -> Self {
        let raw: Vec<(String, String)> = BUILTIN.iter().map(|(n, s)| (format!("{n}.json"), s.to_string())).collect();
        let set = Self::from_sources(raw);
        assert!(set.errors.is_empty(), "内置主题有错: {:?}", set.errors);
        set
    }

    /// 内置主题加上 `dir` 里的 `*.json`。用户主题可以 `extends` 内置主题，也可以同名覆盖。
    pub fn load(dir: &Path) -> Self {
        let mut raw: Vec<(String, String)> =
            BUILTIN.iter().map(|(n, s)| (format!("{n}.json"), s.to_string())).collect();
        let mut errors = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for p in files.into_iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
                match std::fs::read_to_string(&p) {
                    Ok(s) => raw.push((p.file_name().unwrap().to_string_lossy().into_owned(), s)),
                    Err(e) => errors.push(format!("{}: {e}", p.display())),
                }
            }
        }
        let mut set = Self::from_sources(raw);
        set.errors.splice(0..0, errors);
        set
    }

    fn from_sources(sources: Vec<(String, String)>) -> Self {
        let mut values: HashMap<String, (String, Value)> = HashMap::new();
        let mut order = Vec::new();
        let mut errors = Vec::new();
        for (file, src) in sources {
            match jsonc::parse::<Value>(&file, &src) {
                Ok(v) => {
                    let name = v
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| file.trim_end_matches(".json").to_string());
                    if !order.contains(&name) {
                        order.push(name.clone());
                    }
                    values.insert(name, (file, v));
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        let mut themes = BTreeMap::new();
        for name in order {
            match resolve(&name, &values, &mut Vec::new()) {
                Ok(v) => {
                    let file = &values[&name].0;
                    match serde_json::from_value::<Theme>(v) {
                        Ok(mut t) => {
                            t.name = name.clone();
                            match t.components() {
                                Ok(_) => {
                                    themes.insert(name, t);
                                }
                                Err(e) => errors.push(format!("{file}: {e}")),
                            }
                        }
                        Err(e) => errors.push(format!("{file}: {e}")),
                    }
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        Self { themes, errors }
    }

    pub fn get(&self, name: &str) -> Result<&Theme> {
        self.themes.get(name).ok_or_else(|| Error::UnknownTheme(name.into()))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.themes.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Theme> {
        self.themes.values()
    }
}

/// 把 `extends` 链展开成一个完整的主题 JSON。
fn resolve(name: &str, all: &HashMap<String, (String, Value)>, stack: &mut Vec<String>) -> Result<Value> {
    if stack.iter().any(|s| s == name) {
        stack.push(name.into());
        return Err(Error::ExtendsCycle(stack.join(" → ")));
    }
    let (_, v) = all.get(name).ok_or_else(|| Error::UnknownTheme(name.into()))?;
    let Some(parent) = v.get("extends").and_then(Value::as_str) else {
        return Ok(v.clone());
    };
    stack.push(name.into());
    let mut base = resolve(parent, all, stack)?;
    stack.pop();
    merge(&mut base, v);
    if let Value::Object(m) = &mut base {
        m.remove("extends");
    }
    Ok(base)
}

fn merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_load_and_pass_contrast() {
        let set = ThemeSet::builtin();
        assert_eq!(set.names().count(), 5);
        for t in set.iter() {
            assert_eq!(t.check_contrast().unwrap(), vec![], "{}", t.name);
        }
        assert_eq!(set.get(DEFAULT_LIGHT).unwrap().kind, Kind::Light);
        assert_eq!(set.get(DEFAULT_DARK).unwrap().kind, Kind::Dark);
    }

    #[test]
    fn components_follow_rules_and_overrides() {
        let mut t = ThemeSet::builtin().get("moon-paper").unwrap().clone();
        let c = t.components().unwrap();
        assert_eq!(c.len(), components_table().len());
        assert_eq!(c["tab.indicator"], t.ui.accent);
        assert_eq!(c["scrollbar.thumb"], format!("{}40", t.ui.text));
        t.overrides.insert("tab.indicator".into(), "#FF0000".into());
        assert_eq!(t.components().unwrap()["tab.indicator"], "#FF0000");
        t.overrides.insert("tab.indicator".into(), "red".into());
        assert!(t.components().is_err());
    }

    #[test]
    fn user_theme_extends_builtin() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("my.json"),
            r##"{
                // 只改强调色
                "name": "my-moon", "displayName": "我的月白", "extends": "moon-paper",
                "ui": { "accent": "#3D5A99" },
            }"##,
        )
        .unwrap();
        std::fs::write(dir.path().join("broken.json"), "{ not json").unwrap();
        let set = ThemeSet::load(dir.path());
        let t = set.get("my-moon").unwrap();
        assert_eq!(t.ui.accent, "#3D5A99");
        assert_eq!(t.ui.surface0, set.get("moon-paper").unwrap().ui.surface0);
        assert_eq!(t.display_name, "我的月白");
        assert_eq!(set.errors.len(), 1, "{:?}", set.errors);
        assert!(set.errors[0].contains("broken.json"));
    }

    #[test]
    fn extends_cycle_is_reported() {
        let set = ThemeSet::from_sources(vec![
            ("a.json".into(), r#"{"name":"a","extends":"b"}"#.into()),
            ("b.json".into(), r#"{"name":"b","extends":"a"}"#.into()),
        ]);
        assert!(set.errors.iter().any(|e| e.contains("循环")), "{:?}", set.errors);
    }
}
