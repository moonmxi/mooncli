//! 伪终端会话和 shell 启动。

pub mod session;
pub mod shell;

pub use session::{Item, Session, Sink, SpawnOptions};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("伪终端出错: {0}")]
    Pty(String),
    #[error("启动 {0} 失败: {1}")]
    Spawn(String, String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    fn pty(e: impl std::fmt::Display) -> Self {
        Error::Pty(e.to_string())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for Error {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error::Pty(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
