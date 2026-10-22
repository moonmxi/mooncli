//! Agent 用量数据层。设计见 docs/07-agent-monitor.md。
//!
//! 只读两类数据（合规边界，docs/07 第 1 节）：
//! 1. 官方扩展点送来的：Claude Code 的 statusLine / hooks、Codex 的 notify——由 mooncli-bridge
//!    写进 mooncli 自己的数据目录（spool），主进程从那里读。
//! 2. 工具自己写在本机的文件：Claude Code 的会话转录、Codex 的 rollout 会话文件。
//!
//! 绝不读取任何凭证，绝不发网络请求。

pub mod claude;
pub mod codex;
pub mod integrate;
mod jsonl;
pub mod model;
pub mod spool;
mod timeutil;

pub use model::{AccountUsage, AgentKind, ContextUsage, RateWindow, SessionState, SessionUsage, SnapshotSource, UsageReport};
pub use timeutil::now_epoch;

use std::path::PathBuf;

/// 数据来源开关，对应设置里的三个子开关（docs/07 第 6 节）。
#[derive(Debug, Clone, Copy)]
pub struct Sources {
    /// 读取会话文件（Claude 转录、Codex rollout）
    pub files: bool,
    /// 接收 statusLine 数据（读 spool）
    pub statusline: bool,
    /// 接收 hooks / notify 的状态（读 spool 的 hook 条目；Codex 会话文件本身不受影响）
    pub hooks: bool,
}

impl Default for Sources {
    fn default() -> Self {
        Self { files: true, statusline: true, hooks: true }
    }
}

/// 一次采集要看的目录。主进程按自己的数据目录约定填好，测试里用临时目录。
#[derive(Debug, Clone)]
pub struct CollectOptions {
    /// Claude Code 配置目录（默认 ~/.claude，可被 CLAUDE_CONFIG_DIR 改）
    pub claude_config_dir: PathBuf,
    /// Codex 数据目录（可多个）
    pub codex_homes: Vec<PathBuf>,
    /// mooncli 的 agent 数据目录（bridge 的 spool 在其下）
    pub agent_dir: PathBuf,
    pub sources: Sources,
}

impl CollectOptions {
    pub fn defaults(agent_dir: PathBuf) -> Self {
        Self {
            claude_config_dir: claude::claude_config_dir(),
            codex_homes: codex::codex_homes(),
            agent_dir,
            sources: Sources::default(),
        }
    }
}

/// 采集全部用量数据。任何一处读不到都只会让对应部分为空，不报错——小窗宁可显示「不可用」。
pub fn collect_all(opts: &CollectOptions) -> UsageReport {
    let mut report = UsageReport::default();

    if opts.sources.files {
        let (codex_sessions, codex_account) = codex::collect(&opts.codex_homes);
        report.sessions.extend(codex_sessions);
        if let Some(account) = codex_account {
            report.accounts.push(account);
        }
    }

    let (claude_sessions, claude_account) = claude::collect(&opts.claude_config_dir, &opts.agent_dir, opts.sources);
    report.sessions.extend(claude_sessions);
    if let Some(account) = claude_account {
        report.accounts.push(account);
    }

    // 每个 kind 内按更新时间倒序，会话级在前
    report.sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    report
}
