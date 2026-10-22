//! 小窗的数据模型。和界面（app/src/agent/types.ts）一一对应，序列化成 camelCase JSON。
//!
//! 合规边界见 docs/07 第 1 节：这里出现的数据只能来自
//! 1. 工具官方扩展点送来的内容（Claude Code statusLine / hooks、Codex notify），
//! 2. 工具自己写在本机的文件（会话记录）。
//! 不出现任何凭证，不发任何网络请求。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
}

/// 一个额度窗口（5 小时、周等）。窗口语义认 `window_minutes`，不按套餐写死：
/// Codex 的 primary 在 Pro 套餐上可能缺席，Claude 的窗口也可能随官方调整。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    /// 已用百分比 0–100（后端/官方给的数，不用 token 数自行推算）
    pub used_percent: f64,
    /// 窗口长度（分钟）：300 = 5 小时，10080 = 一周；拿不到时为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
    /// 窗口重置时刻（Unix epoch 秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
    /// true = 本地估算（Claude 拿不到官方窗口时的 token 汇总），不是官方百分比
    #[serde(default)]
    pub estimated: bool,
}

/// 会话级上下文占用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    /// 当前上下文里的 token 数（最近一次请求的输入侧总量）
    pub used_tokens: i64,
    /// 模型上下文上限
    pub window_tokens: i64,
}

impl ContextUsage {
    pub fn percent(&self) -> f64 {
        if self.window_tokens <= 0 {
            return 0.0;
        }
        (self.used_tokens as f64 / self.window_tokens as f64 * 100.0).clamp(0.0, 100.0)
    }
}

/// 数据出处，界面据此决定要不要标「估算」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotSource {
    /// 官方扩展点（statusLine）实时送来的
    StatusLine,
    /// 工具自己写的会话文件里的快照
    SessionFile,
    /// 本地汇总估算（没有官方百分比）
    Estimate,
}

/// 一个会话的用量快照（会话级数据：上下文、模型、目录）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    pub kind: AgentKind,
    /// 会话 ID：Claude 是 session_id，Codex 是 thread id（= rollout 文件名尾部的 UUID）
    pub session_id: String,
    /// mooncli 标签页的会话 ID（MOONCLI_SESSION_ID）；只读文件发现的会话没有这个字段，
    /// 界面退而按 cwd 对应标签页
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextUsage>,
    /// 本次会话累计花费（美元）；只有 Claude statusLine 给得出，且只对 API 计费有意义
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// 会话状态：running / waiting（等用户确认）/ done / unknown——来自 hooks / notify，
    /// 只有文件数据时为 unknown
    pub state: SessionState,
    /// 快照时刻（epoch 秒）：文件 mtime 或扩展点送达时刻，界面用来标新鲜度
    pub updated_at: i64,
    pub source: SnapshotSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Running,
    Waiting,
    Done,
    Unknown,
}

/// 账号级用量（额度窗口是账号级的，多个会话共用一组）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    pub kind: AgentKind,
    /// 主窗口（通常是 5 小时；可能缺席，如 Codex Pro 无 5 小时窗口）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<RateWindow>,
    /// 次窗口（通常是周）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<RateWindow>,
    /// 套餐（Codex 的 plan_type 原样透传，如 plus / pro）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// 估算模式下当前窗口内累计的 token 数（Claude 无官方窗口时显示这个而不是百分比）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_tokens: Option<i64>,
    pub updated_at: i64,
    pub source: SnapshotSource,
}

/// 一次采集的全部结果，Tauri 命令 `agent_usage` 的返回。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub sessions: Vec<SessionUsage>,
    pub accounts: Vec<AccountUsage>,
}
