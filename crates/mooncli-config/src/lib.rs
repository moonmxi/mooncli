//! 设置与主题。设计见 docs/03-appearance.md。

pub mod color;
pub mod jsonc;
pub mod settings;
pub mod theme;

pub use settings::Settings;
pub use theme::{Theme, ThemeSet};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{file}: 第 {line} 行第 {column} 列: {msg}")]
    Parse { file: String, line: usize, column: usize, msg: String },
    #[error("主题 {0} 不存在")]
    UnknownTheme(String),
    #[error("主题继承形成了循环: {0}")]
    ExtendsCycle(String),
    #[error("颜色格式不对: {0}（应为 #RRGGBB 或 #RRGGBBAA）")]
    BadColor(String),
    #[error("读取文件失败: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
