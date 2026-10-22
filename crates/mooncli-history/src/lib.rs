//! 命令历史库。设计见 docs/04-command-memory.md 第 2、3 节。

mod filter;
mod import;
mod index;
mod search;
mod store;

pub use filter::Filter;
pub use import::parse_psreadline_history;
pub use search::{Hit, Query, Scope, Sort};
pub use store::{CommandRecord, HostCount, Run, Source, Store, Summary};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("过滤规则写错了: {0}")]
    BadRule(#[from] regex::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
