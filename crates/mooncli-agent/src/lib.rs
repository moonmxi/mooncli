//! Agent 用量与完成提醒（Claude Code 与 Codex）。设计见 docs/07-agent-monitor.md。
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
pub mod claude_usage;
pub mod codex;
pub mod codex_live;
pub mod integrate;
mod jsonl;
pub mod model;
pub mod notify;
pub mod spool;
mod timeutil;

pub use model::{AccountUsage, AgentKind, ContextUsage, CurrentSession, RateWindow, SnapshotSource, UsageReport};
pub use timeutil::now_epoch;

use std::path::PathBuf;

/// 一次采集要看的目录。主进程按自己的数据目录约定填好，测试里用临时目录。
#[derive(Debug, Clone)]
pub struct CollectOptions {
    /// Codex 数据目录（可多个，见 CODEX_HOME）
    pub codex_homes: Vec<PathBuf>,
    /// Claude Code 配置目录（CLAUDE_CONFIG_DIR 或 ~/.claude）
    pub claude_dir: PathBuf,
    /// mooncli 的 agent 数据目录（bridge 的 spool 在其下）
    pub agent_dir: PathBuf,
}

impl CollectOptions {
    pub fn defaults(agent_dir: PathBuf) -> Self {
        Self { codex_homes: codex::codex_homes(), claude_dir: claude::claude_config_dir(), agent_dir }
    }
}

/// 当前标签页正在运行的 Agent：只为它找上下文占用，其它会话一概不看。
#[derive(Debug, Clone)]
pub struct Focus {
    pub kind: AgentKind,
    /// mooncli 标签页会话 ID（bridge 从 MOONCLI_SESSION_ID 记下，用来认出这个标签页的会话）
    pub tab_id: String,
    /// 标签页当前目录：没接入扩展点时按目录找会话文件
    pub cwd: Option<String>,
    /// Agent 开始运行的时刻（epoch 秒）：更早的会话记录不算
    pub since: i64,
}

/// 采集额度与当前会话。任何一处读不到都只会让对应部分为空，不报错。
/// `live` 是主进程查到的额度（`claude /usage`、codex app-server）；每个 Agent 在实时查询、
/// 扩展点送来的、会话文件里的几份数据中取最新的一份——一轮对话刚结束时文件里的往往比上次查询新。
pub fn collect(opts: &CollectOptions, live: &[AccountUsage], focus: Option<&Focus>) -> UsageReport {
    let freshest =
        |candidates: Vec<Option<AccountUsage>>| candidates.into_iter().flatten().max_by_key(|a| a.updated_at);
    let live_of = |kind: AgentKind| live.iter().find(|a| a.kind == kind).cloned();
    let accounts = [
        freshest(vec![live_of(AgentKind::Claude), claude::latest_account(&opts.agent_dir)]),
        freshest(vec![live_of(AgentKind::Codex), codex::latest_account(&opts.codex_homes)]),
    ]
    .into_iter()
    .flatten()
    .collect();
    let current = focus.map(|f| match f.kind {
        AgentKind::Claude => claude::current(&opts.claude_dir, &opts.agent_dir, f),
        AgentKind::Codex => codex::current(&opts.codex_homes, &opts.agent_dir, f),
    });
    UsageReport { accounts, current }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_codex_quota_is_used_and_current_follows_focus() {
        let dir = tempfile::tempdir().unwrap();
        let opts = CollectOptions {
            codex_homes: vec![dir.path().join("codex")],
            claude_dir: dir.path().join("claude"),
            agent_dir: dir.path().join("agent"),
        };
        let live = AccountUsage::from_windows(
            AgentKind::Codex,
            Some(RateWindow { used_percent: 5.0, window_minutes: Some(300), resets_at: None }),
            None,
            Some("plus".into()),
            50,
            SnapshotSource::Live,
        )
        .unwrap();
        let r = collect(&opts, std::slice::from_ref(&live), None);
        assert_eq!(r.accounts, vec![live.clone()]);
        // 会话文件里有更新的快照时用文件里的
        let day = dir.path().join("codex").join("sessions").join("2026").join("10").join("26");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join("rollout-x.jsonl"),
            concat!(
                r#"{"type":"session_meta","payload":{"id":"x","cwd":"C:\\w"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":9.5,"window_minutes":300}}}}"#,
                "\n"
            ),
        )
        .unwrap();
        let r = collect(&opts, std::slice::from_ref(&live), None);
        assert_eq!(r.accounts[0].five_hour.as_ref().unwrap().used_percent, 9.5);
        assert_eq!(r.accounts[0].source, SnapshotSource::SessionFile);
        assert!(r.current.is_none());
        let focus = Focus { kind: AgentKind::Codex, tab_id: "t".into(), cwd: None, since: 0 };
        let r = collect(&opts, &[live], Some(&focus));
        let cur = r.current.unwrap();
        assert_eq!(cur.kind, AgentKind::Codex);
        assert!(cur.context.is_none());
    }
}
