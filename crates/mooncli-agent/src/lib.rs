//! Agent 用量数据层（Codex）。设计见 docs/07-agent-monitor.md。
//!
//! 只读两类数据（合规边界，docs/07 第 1 节）：
//! 1. Codex 自己写在本机的会话文件（rollout JSONL）——上下文占用、token 用量，
//!    以及后端下发并落盘的额度快照（rate_limits，与官方 /status 同源）；
//! 2. 官方扩展点 notify 送来的事件——由 mooncli-bridge 写进 mooncli 自己的
//!    数据目录（spool），主进程从那里读，只用来标会话状态和加速刷新。
//!
//! 绝不读取任何凭证，绝不发网络请求。

pub mod codex;
pub mod integrate;
mod jsonl;
pub mod model;
pub mod spool;
mod timeutil;

pub use model::{AccountUsage, AgentKind, ContextUsage, RateWindow, SessionState, SessionUsage, UsageReport};
pub use timeutil::now_epoch;

use std::path::PathBuf;

/// 数据来源开关，对应设置里的两个子开关（docs/07 第 6 节）。
#[derive(Debug, Clone, Copy)]
pub struct Sources {
    /// 读取 Codex 会话文件（rollout JSONL）
    pub files: bool,
    /// 接收 notify 状态（读 spool 里的 codex-notify 条目）
    pub notify: bool,
}

impl Default for Sources {
    fn default() -> Self {
        Self { files: true, notify: true }
    }
}

/// 一次采集要看的目录。主进程按自己的数据目录约定填好，测试里用临时目录。
#[derive(Debug, Clone)]
pub struct CollectOptions {
    /// Codex 数据目录（可多个，见 CODEX_HOME）
    pub codex_homes: Vec<PathBuf>,
    /// mooncli 的 agent 数据目录（bridge 的 spool 在其下）
    pub agent_dir: PathBuf,
    pub sources: Sources,
}

impl CollectOptions {
    pub fn defaults(agent_dir: PathBuf) -> Self {
        Self { codex_homes: codex::codex_homes(), agent_dir, sources: Sources::default() }
    }
}

/// 采集全部用量数据。任何一处读不到都只会让对应部分为空，不报错——小窗宁可显示「不可用」。
pub fn collect_all(opts: &CollectOptions) -> UsageReport {
    let mut report = UsageReport::default();
    if opts.sources.files {
        let agent_dir = opts.sources.notify.then_some(opts.agent_dir.as_path());
        let (sessions, account) = codex::collect(&opts.codex_homes, agent_dir);
        report.sessions = sessions;
        if let Some(account) = account {
            report.accounts.push(account);
        }
    }
    report.sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    report
}
