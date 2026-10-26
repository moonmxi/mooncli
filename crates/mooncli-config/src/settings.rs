//! settings.json 的结构和默认值。每个字段都有默认值，文件里只写想改的部分。
//! 字段含义见 docs/03（外观）、docs/04（历史）、docs/06（标签栏）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Result, jsonc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemeChoice,
    /// 整体缩放（界面和终端一起），Ctrl+= / Ctrl+- / Ctrl+0
    pub zoom: f32,
    /// 只缩放标题栏、侧栏、状态栏这些界面部分，和终端字号互不影响
    pub ui_scale: f32,
    pub font: Font,
    pub background: Background,
    pub window: Window,
    pub tabs: Tabs,
    pub terminal: Terminal,
    pub shell: Shell,
    pub history: History,
    pub remote: Remote,
    pub agent: Agent,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::default(),
            zoom: 1.0,
            ui_scale: 1.0,
            font: Font::default(),
            background: Background::default(),
            window: Window::default(),
            tabs: Tabs::default(),
            terminal: Terminal::default(),
            shell: Shell::default(),
            history: History::default(),
            remote: Remote::default(),
            agent: Agent::default(),
        }
    }
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

/// 默认终端字体：英文 → Nerd Font 图标 → 中文 → 彩色 emoji → 符号 → 系统等宽字体兜底（docs/03 §2）。
/// 和 app/src/state/settings.ts 里的保持一致。
/// "mooncli Symbols" 是程序附带的 Nerd Font 符号字体，保证提示符美化工具的图标总能显示。
pub const DEFAULT_FONT_FAMILY: &str = r#""Cascadia Code", "Cascadia Mono", Consolas, "mooncli Symbols", "Microsoft YaHei UI", "Segoe UI Emoji", "Segoe UI Symbol", monospace, "Noto Color Emoji""#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Font {
    /// CSS 写法的字体回退链
    pub family: String,
    pub size: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub weight: String,
    pub weight_bold: String,
    pub ligatures: bool,
    pub ui_family: String,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: DEFAULT_FONT_FAMILY.into(),
            size: 14.0,
            line_height: 1.25,
            letter_spacing: 0.0,
            weight: "normal".into(),
            weight_bold: "bold".into(),
            ligatures: true,
            ui_family: r#""Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei UI", sans-serif"#.into(),
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
    /// 右键：`copyPaste`（有选中就复制，否则粘贴）或 `menu`（弹出菜单）
    pub right_click: String,
    /// 复制后弹出"已复制"小卡片
    pub copy_toast: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            scrollback: 10_000,
            minimum_contrast_ratio: 3.0,
            cursor_style: "bar".into(),
            cursor_blink: true,
            smooth_scroll_ms: 120,
            copy_on_select: false,
            trim_trailing_whitespace: true,
            confirm_multiline_paste: true,
            right_click: "copyPaste".into(),
            copy_toast: true,
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
    /// 让 PSReadLine 和 $PSStyle 的配色跟随主题
    pub adapt_colors: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self { path: None, args: Vec::new(), integration: true, use_bundled_conpty: true, adapt_colors: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub enabled: bool,
    /// 额外的"不记录"正则
    pub ignore_patterns: Vec<String>,
    pub import_psreadline: bool,
    /// 只保留最近多少天，0 表示一直保留。启动时清理一次
    pub keep_days: u32,
}

impl Default for History {
    fn default() -> Self {
        Self { enabled: true, ignore_patterns: Vec::new(), import_psreadline: true, keep_days: 0 }
    }
}

/// 远程连接的识别（docs/06 §4.1）
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Remote {
    /// 除了内置的 ssh、sshp、mosh、wsl……之外，也按 ssh 的写法识别成连接的命令（比如自己的别名）
    pub connect_commands: Vec<String>,
}

/// Agent（docs/07，Claude Code 与 Codex）：额度卡片默认关闭，完成提醒在接入后默认开启。
/// 卡片的摆放、样式等只在界面侧用得到的字段不进这个结构（设置以 JSON 原样保存，不会丢）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Agent {
    /// 额度卡片的总开关。关时不采集、不显示卡片
    pub enabled: bool,
    /// 主动查询额度：`claude -p /usage`、codex app-server（登录、联网都由它们自己完成）
    pub live_quota: bool,
    /// claude 可执行文件的位置；不填时自动在 PATH 里找
    pub claude_path: Option<String>,
    /// codex 可执行文件的位置；不填时自动在 PATH 里找
    pub codex_path: Option<String>,
    pub notify: AgentNotify,
}

impl Default for Agent {
    fn default() -> Self {
        Self { enabled: false, live_quota: true, claude_path: None, codex_path: None, notify: AgentNotify::default() }
    }
}

/// 完成提醒（Claude Code / Codex 一轮结束、要你确认时弹系统通知）。要先在设置里接入 hooks / notify。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentNotify {
    pub enabled: bool,
    /// 你正看着那个标签页时不弹
    pub quiet_when_focused: bool,
}

impl Default for AgentNotify {
    fn default() -> Self {
        Self { enabled: true, quiet_when_focused: true }
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
        assert_eq!(s.zoom, 1.0);
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
