//! 小窗的数据模型。和界面（app/src/agent/usage.ts、backend/types.ts）一一对应，
//! 序列化成 camelCase JSON。
//!
//! 合规边界见 docs/07 第 1 节：这里出现的数据只能来自
//! 1. Codex 官方扩展点（notify）送来的内容，
//! 2. Codex 自己写在本机的会话文件（rollout JSONL 里的用量与额度快照）。
//!
//! 不出现任何凭证，不发任何网络请求。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Codex,
}

/// 一个额度窗口（5 小时、周等）。窗口语义认 `window_minutes`，不按套餐写死：
/// 任一窗口都可能缺席（如 Pro 套餐没有 5 小时窗口）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    /// 已用百分比 0–100（后端下发、Codex 落盘的快照值，不用 token 数自行推算）
    pub used_percent: f64,
    /// 窗口长度（分钟）：300 = 5 小时，10080 = 一周；拿不到时为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
    /// 窗口重置时刻（Unix epoch 秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
    /// 预留：本地估算窗口为 true。Codex 的窗口都来自官方快照，恒为 false
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

/// 一个会话的用量快照（会话级数据：上下文、模型、目录）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    pub kind: AgentKind,
    /// 会话 ID：Codex 的 thread id（= rollout 文件名尾部的 UUID）
    pub session_id: String,
    /// mooncli 标签页的会话 ID（MOONCLI_SESSION_ID）；只读文件发现的会话没有这个字段，
    /// 界面退而按 cwd 对应标签页。接入 notify 后，bridge 会把两者关联起来。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextUsage>,
    /// 会话状态：notify 报过一轮结束（agent-turn-complete）的是 done，其余 unknown
    pub state: SessionState,
    /// 快照时刻（epoch 秒）：文件 mtime 与 notify 送达时刻取较晚者，界面用来标新鲜度
    pub updated_at: i64,
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
    /// 主窗口（通常是 5 小时；可能缺席，如 Pro 无 5 小时窗口）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<RateWindow>,
    /// 次窗口（通常是周）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<RateWindow>,
    /// 套餐（Codex 的 plan_type 原样透传，如 plus / pro）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub updated_at: i64,
}

/// 一次采集的全部结果，Tauri 命令 `agent_usage` 的返回。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub sessions: Vec<SessionUsage>,
    pub accounts: Vec<AccountUsage>,
}
