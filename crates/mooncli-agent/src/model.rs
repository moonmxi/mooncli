//! 用量卡片的数据模型。和界面（app/src/agent/usage.ts、backend/types.ts）一一对应，
//! 序列化成 camelCase JSON。
//!
//! 合规边界见 docs/07 第 1 节：这里出现的数据只能来自
//! 1. 官方扩展点送来的内容（Claude Code 的 statusLine / hooks，Codex 的 notify），
//! 2. 官方客户端自己的接口（`codex app-server` 的 account/rateLimits/read），
//! 3. 两者自己写在本机的会话文件。
//!
//! 不出现任何凭证；mooncli 自己不发网络请求。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
}

/// 数据从哪来，界面据此标注新鲜度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotSource {
    /// Claude Code 的 statusLine 输入（官方扩展点，每次刷新状态栏都会送来）
    StatusLine,
    /// 官方客户端接口实时查询（codex app-server 的 account/rateLimits/read）
    Live,
    /// 会话文件里最近一次落盘的快照
    SessionFile,
}

/// 一个额度窗口。窗口语义认 `window_minutes`（300 = 5 小时，10080 = 7 天），不按套餐写死。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    /// 已用百分比 0–100（官方下发的值，不用 token 数自行推算）
    pub used_percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
    /// 窗口重置时刻（Unix epoch 秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
}

/// 账号额度。5 小时与 7 天两个窗口都可能缺席（比如 Codex Pro 没有 5 小时窗口）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    pub kind: AgentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<RateWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day: Option<RateWindow>,
    /// 套餐（Codex 的 plan_type 原样透传，如 plus / pro；Claude Code 不提供）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub updated_at: i64,
    pub source: SnapshotSource,
}

impl AccountUsage {
    /// 按窗口长度把两个窗口放到对应位置：官方数据里 primary / secondary 的含义随套餐变化，
    /// 认 window_minutes 最可靠；没写长度时按 primary = 5 小时、secondary = 7 天。
    pub fn from_windows(
        kind: AgentKind,
        primary: Option<RateWindow>,
        secondary: Option<RateWindow>,
        plan: Option<String>,
        updated_at: i64,
        source: SnapshotSource,
    ) -> Option<Self> {
        let mut five_hour = None;
        let mut seven_day = None;
        for (w, default_long) in [(primary, false), (secondary, true)] {
            let Some(w) = w else { continue };
            let long = w.window_minutes.map(|m| m > 300).unwrap_or(default_long);
            let slot = if long { &mut seven_day } else { &mut five_hour };
            if slot.is_none() {
                *slot = Some(w);
            }
        }
        (five_hour.is_some() || seven_day.is_some()).then_some(Self {
            kind,
            five_hour,
            seven_day,
            plan,
            updated_at,
            source,
        })
    }
}

/// 当前会话的上下文占用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    /// 当前上下文里的 token 数
    pub used_tokens: i64,
    /// 模型上下文上限
    pub window_tokens: i64,
    /// 已用百分比，和 Agent 自己显示的口径一致（Claude Code 用 statusLine 给的值，Codex 扣掉固定开销）
    pub used_percent: f64,
}

impl ContextUsage {
    pub fn simple(used_tokens: i64, window_tokens: i64) -> Self {
        let used_percent =
            if window_tokens > 0 { (used_tokens as f64 / window_tokens as f64 * 100.0).clamp(0.0, 100.0) } else { 0.0 };
        Self { used_tokens, window_tokens, used_percent }
    }
}

/// 当前标签页里正在运行的那个 Agent 会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentSession {
    pub kind: AgentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 还没有第一轮回复时为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextUsage>,
    pub updated_at: i64,
}

/// 一次采集的结果：两个 Agent 的账号额度 + 当前标签页会话（没在跑 Agent 时为空）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub accounts: Vec<AccountUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<CurrentSession>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(pct: f64, mins: Option<i64>) -> RateWindow {
        RateWindow { used_percent: pct, window_minutes: mins, resets_at: None }
    }

    #[test]
    fn windows_go_to_slots_by_length() {
        let a = AccountUsage::from_windows(
            AgentKind::Codex,
            Some(w(1.0, Some(10080))),
            None,
            None,
            0,
            SnapshotSource::Live,
        )
        .unwrap();
        assert!(a.five_hour.is_none());
        assert_eq!(a.seven_day.unwrap().used_percent, 1.0);
        let a = AccountUsage::from_windows(
            AgentKind::Claude,
            Some(w(2.0, None)),
            Some(w(3.0, None)),
            None,
            0,
            SnapshotSource::StatusLine,
        )
        .unwrap();
        assert_eq!((a.five_hour.unwrap().used_percent, a.seven_day.unwrap().used_percent), (2.0, 3.0));
        assert!(AccountUsage::from_windows(AgentKind::Claude, None, None, None, 0, SnapshotSource::Live).is_none());
    }
}
