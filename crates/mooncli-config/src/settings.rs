//! settings.json 的结构和默认值。每个字段都有默认值，文件里只写想改的部分。
//! 字段含义见 docs/03（外观）、docs/04（历史）、docs/06（标签栏）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Result, jsonc};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub font: Font,
    pub background: Background,
    pub window: Window,
    pub tabs: Tabs,
    pub terminal: Terminal,
    pub shell: Shell,
    pub history: History,
}

/// `"theme": "frost"`，或 `"theme": { "light": …, "dark": …, "follow": "system" }`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ThemeChoice {
    Fixed(String),
    Pair {
        light: String,
        dark: String,
        #[serde(default)]
        follow: Follow,
    },
}

impl Default for ThemeChoice {
    fn default() -> Self {
        ThemeChoice::Pair {
            light: crate::theme::DEFAULT_LIGHT.into(),
            dark: crate::theme::DEFAULT_DARK.into(),
            follow: Follow::Off,
        }
    }
}

impl ThemeChoice {
    /// 按系统当前是否深色决定用哪个主题。
    pub fn pick(&self, system_dark: bool) -> &str {
        match self {
            ThemeChoice::Fixed(n) => n,
            ThemeChoice::Pair { light, dark, follow } => match follow {
                Follow::System if system_dark => dark,
                _ => light,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Follow {
    System,
    /// 默认不跟随：浅色优先
    #[default]
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Font {
    pub family: Vec<String>,
    pub size: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub weight: String,
    pub weight_bold: String,
    pub ligatures: bool,
    pub ui_family: Vec<String>,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: vec!["Cascadia Code".into(), "Microsoft YaHei UI".into(), "Symbols Nerd Font Mono".into()],
            size: 14.0,
            line_height: 1.25,
            letter_spacing: 0.0,
            weight: "normal".into(),
            weight_bold: "bold".into(),
            ligatures: true,
            ui_family: vec!["Segoe UI Variable Text".into(), "Microsoft YaHei UI".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Background {
    pub image: Option<String>,
    pub fit: String,
    pub position: String,
    pub opacity: f32,
    pub blur: f32,
    pub brightness: f32,
    pub saturation: f32,
    pub overlay: f32,
    pub scope: String,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            image: None,
            fit: "cover".into(),
            position: "bottom right".into(),
            opacity: 0.35,
            blur: 0.0,
            brightness: 1.0,
            saturation: 1.0,
            overlay: 0.6,
            scope: "terminal".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Window {
    pub material: String,
    pub opacity: f32,
    pub padding: [u16; 2],
    pub startup_size: [u16; 2],
    pub remember_position: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            material: "mica".into(),
            opacity: 1.0,
            padding: [12, 16],
            startup_size: [120, 32],
            remember_position: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TabsPosition {
    #[default]
    Left,
    Top,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Tabs {
    pub position: TabsPosition,
    pub sidebar: Sidebar,
    pub title_source: String,
}

impl Default for Tabs {
    fn default() -> Self {
        Self { position: TabsPosition::Left, sidebar: Sidebar::default(), title_source: "auto".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Sidebar {
    /// 展开时的宽度，160–360；52 表示收起
    pub width: u16,
    /// 窗口变窄时自动收起（docs/06 §1.2）
    pub auto_collapse: bool,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self { width: 220, auto_collapse: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Terminal {
    pub scrollback: u32,
    pub minimum_contrast_ratio: f32,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub smooth_scroll_ms: u32,
    pub copy_on_select: bool,
    pub trim_trailing_whitespace: bool,
    pub confirm_multiline_paste: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            scrollback: 10_000,
            minimum_contrast_ratio: 4.5,
            cursor_style: "bar".into(),
            cursor_blink: true,
            smooth_scroll_ms: 120,
            copy_on_select: false,
            trim_trailing_whitespace: true,
            confirm_multiline_paste: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Shell {
    /// pwsh.exe 的路径；不填则自动查找
    pub path: Option<String>,
    pub args: Vec<String>,
    /// 关掉后智能功能降级，终端照常可用
    pub integration: bool,
    pub use_bundled_conpty: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self { path: None, args: Vec::new(), integration: true, use_bundled_conpty: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub enabled: bool,
    /// 额外的"不记录"正则
    pub ignore_patterns: Vec<String>,
    pub import_psreadline: bool,
}

impl Default for History {
    fn default() -> Self {
        Self { enabled: true, ignore_patterns: Vec::new(), import_psreadline: true }
    }
}

impl Settings {
    pub fn parse(file: &str, src: &str) -> Result<Self> {
        jsonc::parse(file, src)
    }

    /// 文件不存在时返回默认设置。
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&path.display().to_string(), &s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_is_all_defaults() {
        assert_eq!(Settings::parse("s", "{}").unwrap(), Settings::default());
        assert_eq!(Settings::default().theme.pick(true), "moon-paper");
    }

    #[test]
    fn partial_settings_keep_other_defaults() {
        let s = Settings::parse(
            "s",
            r#"{
                // 用霜蓝，侧栏宽一点
                "theme": "frost",
                "tabs": { "sidebar": { "width": 280 } },
                "font": { "size": 15 },
            }"#,
        )
        .unwrap();
        assert_eq!(s.theme.pick(false), "frost");
        assert_eq!(s.tabs.sidebar.width, 280);
        assert!(s.tabs.sidebar.auto_collapse);
        assert_eq!(s.font.size, 15.0);
        assert_eq!(s.font.line_height, 1.25);
    }

    #[test]
    fn follow_system() {
        let s = Settings::parse("s", r#"{"theme": {"light": "celadon", "dark": "night-voyage", "follow": "system"}}"#)
            .unwrap();
        assert_eq!(s.theme.pick(false), "celadon");
        assert_eq!(s.theme.pick(true), "night-voyage");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(&dir.path().join("nope.json")).unwrap(), Settings::default());
    }
}
