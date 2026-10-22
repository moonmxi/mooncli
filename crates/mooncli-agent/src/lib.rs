//! Agent 用量数据层（Claude Code 与 Codex）。设计见 docs/07-agent-monitor.md。
//!
//! 只用三类数据（合规边界，docs/07 第 1 节）：
//! 1. 官方扩展点送来的数据——Claude Code 的 statusLine / hooks、Codex 的 notify，
//!    由 mooncli-bridge 写进 mooncli 自己的数据目录（spool），主进程从那里读；
//! 2. 官方客户端自己的接口——`codex app-server` 的 account/rateLimits/read
//!    （登录和联网都由用户本机的 Codex 完成，见 codex_live）；
//! 3. 两者自己写在本机的会话文件（Codex rollout、Claude Code 转录）。
//!
//! 绝不读取任何凭证，mooncli 自己不发网络请求。

pub mod claude;
pub mod codex;
pub mod codex_live;
pub mod integrate;
mod jsonl;
pub mod model;
pub mod spool;
mod timeutil;

pub use model::{
    AccountUsage, AgentKind, ContextUsage, RateWindow, SessionState, SessionUsage, SnapshotSource, UsageReport,
};
pub use timeutil::now_epoch;

use std::path::PathBuf;

/// 数据来源开关，对应设置里的子开关（docs/07 第 6 节）。
#[derive(Debug, Clone, Copy)]
pub struct Sources {
    /// 读取会话文件（Codex rollout、Claude Code 转录）
    pub files: bool,
    /// 接收扩展点送来的数据（spool：statusLine、hooks、notify）
    pub extensions: bool,
}

impl Default for Sources {
    fn default() -> Self {
        Self { files: true, extensions: true }
    }
}

/// 一次采集要看的目录。主进程按自己的数据目录约定填好，测试里用临时目录。
#[derive(Debug, Clone)]
pub struct CollectOptions {
    /// Codex 数据目录（可多个，见 CODEX_HOME）
    pub codex_homes: Vec<PathBuf>,
    /// Claude Code 配置目录（CLAUDE_CONFIG_DIR 或 ~/.claude）
    pub claude_dir: PathBuf,
    /// mooncli 的 agent 数据目录（bridge 的 spool 在其下）
    pub agent_dir: PathBuf,
    pub sources: Sources,
}

impl CollectOptions {
    pub fn defaults(agent_dir: PathBuf) -> Self {
        Self {
            codex_homes: codex::codex_homes(),
            claude_dir: claude::claude_config_dir(),
            agent_dir,
            sources: Sources::default(),
        }
    }
}

/// 采集全部用量数据。任何一处读不到都只会让对应部分为空，不报错——小窗宁可显示「不可用」。
/// `live` 是主进程缓存的实时额度（codex app-server 查到的），比会话文件里的快照新时替换它。
pub fn collect_all(opts: &CollectOptions, live: &[AccountUsage]) -> UsageReport {
    let mut report = UsageReport::default();
    let spool = opts.sources.extensions.then_some(opts.agent_dir.as_path());

    let (sessions, account) = claude::collect(&opts.claude_dir, spool, opts.sources.files);
    report.sessions.extend(sessions);
    report.accounts.extend(account);

    if opts.sources.files {
        let (sessions, account) = codex::collect(&opts.codex_homes, spool);
        report.sessions.extend(sessions);
        report.accounts.extend(account);
    }

    for l in live {
        match report.accounts.iter_mut().find(|a| a.kind == l.kind) {
            Some(a) if a.updated_at >= l.updated_at => {}
            Some(a) => *a = l.clone(),
            None => report.accounts.push(l.clone()),
        }
    }
    report.sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_snapshot_wins_only_when_newer() {
        let dir = tempfile::tempdir().unwrap();
        let opts = CollectOptions {
            codex_homes: vec![dir.path().join("codex")],
            claude_dir: dir.path().join("claude"),
            agent_dir: dir.path().join("agent"),
            sources: Sources::default(),
        };
        let live = AccountUsage {
            kind: AgentKind::Codex,
            primary: Some(RateWindow {
                used_percent: 5.0,
                window_minutes: Some(300),
                resets_at: None,
                estimated: false,
            }),
            secondary: None,
            plan: Some("plus".into()),
            updated_at: 50,
            source: SnapshotSource::Live,
        };
        let r = collect_all(&opts, std::slice::from_ref(&live));
        assert_eq!(r.accounts, vec![live]);
    }
}
