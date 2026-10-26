//! Agent 的主进程侧（docs/07）：额度与当前会话的采集、实时额度缓存、完成提醒、bridge 位置、一键接入/断开。
//! 采集、提醒与配置合并的纯逻辑在 mooncli-agent 里，这里负责路径、备份、落盘和弹通知。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use mooncli_agent::integrate;
use mooncli_agent::notify;
use mooncli_agent::{AccountUsage, AgentKind, CollectOptions, CurrentSession, Focus, now_epoch};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};

use crate::AppState;
use crate::files;

/// 多久主动查一次额度（秒）。每次都要起一个 Claude Code / Codex 进程（几秒 CPU），不宜太勤：
/// 平时 Codex 两分钟、Claude Code 五分钟；当前标签页正在跑这个 Agent 时一分半。
const LIVE_INTERVAL_CODEX: i64 = 120;
const LIVE_INTERVAL_CLAUDE: i64 = 300;
const LIVE_INTERVAL_ACTIVE: i64 = 90;
/// Claude Code 的额度接口查得太勤会限流（`/usage` 只回一句"暂时拿不到"），跑着的时候也只三分钟一次
const LIVE_INTERVAL_ACTIVE_CLAUDE: i64 = 180;
/// Agent 刚有动静（一轮结束、状态栏刷新）时立刻重查，但两次之间至少隔这么久（秒）
const LIVE_MIN_GAP: i64 = 30;
const LIVE_MIN_GAP_CLAUDE: i64 = 120;
/// 失败后隔多久再试（秒）：没装、没登录时别反复起进程
const LIVE_RETRY_AFTER_ERROR: i64 = 600;
/// 偶发的失败（额度接口临时限流、网络抖一下）很快再试：第一次失败 1 分钟后、第二次 5 分钟后，之后才隔 10 分钟
const LIVE_RETRY_STEPS: [i64; 2] = [60, 300];
/// spool 文件多久没更新就清掉（秒）
const SPOOL_KEEP: i64 = 7 * 86_400;

#[derive(Default)]
pub struct AgentState {
    live: Mutex<LiveCache>,
}

#[derive(Default)]
struct LiveCache {
    claude: LiveSlot,
    codex: LiveSlot,
    pruned_at: i64,
}

impl LiveCache {
    fn slot(&mut self, kind: AgentKind) -> &mut LiveSlot {
        match kind {
            AgentKind::Claude => &mut self.claude,
            AgentKind::Codex => &mut self.codex,
        }
    }
}

/// 一个 Agent 的主动查询状态
#[derive(Default)]
struct LiveSlot {
    account: Option<AccountUsage>,
    /// 上次开始查询的时刻（epoch 秒）
    last_try: i64,
    last_error: Option<String>,
    /// 连着失败了几次（成功一次就清零）
    failures: usize,
    in_flight: bool,
    /// 上次查询时看到的 spool 最新修改时刻：变了说明 Agent 又跑完了一轮
    seen_activity: i64,
}

/// 哪些 spool 文件的变化算这个 Agent「刚有动静」
fn activity_prefixes(kind: AgentKind) -> &'static [&'static str] {
    match kind {
        AgentKind::Claude => &["claude-hook-", "claude-status-"],
        AgentKind::Codex => &["codex-notify-"],
    }
}

/// mooncli 的 agent 数据目录：data/agent（bridge 的 spool、提醒与 bridge-config 在这里）。
/// bridge 在 mooncli 外面被调用时也按同一个约定找到这里（见 mooncli-bridge 的 default_agent_dir）。
pub fn agent_dir(state: &AppState) -> PathBuf {
    state.data.data_dir.join("agent")
}

/// 界面告诉后端：当前标签页里正在跑哪个 Agent（没在跑就不传）
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusArg {
    kind: AgentKind,
    tab_id: String,
    cwd: Option<String>,
    /// Agent 开始运行的时刻（毫秒）
    since_ms: i64,
}

#[derive(Serialize, Default)]
pub struct LiveErrors {
    #[serde(skip_serializing_if = "Option::is_none")]
    claude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    codex: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    pub accounts: Vec<AccountUsage>,
    pub current: Option<CurrentSession>,
    /// 各 Agent 最近一次主动查询失败的原因（成功或没开时为空）
    pub live_errors: LiveErrors,
    /// 正在查询的 Agent（卡片上显示「查询中」）
    pub refreshing: Vec<AgentKind>,
}

/// 采集两个 Agent 的额度，以及当前标签页里正在跑的那个会话的上下文。
/// 顺手在后台按需重查额度（到点了、或 Agent 刚跑完一轮），本次先用缓存，下一轮轮询就能拿到新的。
/// 异步命令：读文件、扫目录都在后台线程里做，界面线程不等。
#[tauri::command]
pub async fn agent_usage(app: tauri::AppHandle, focus: Option<FocusArg>) -> Result<UsageResponse, String> {
    tauri::async_runtime::spawn_blocking(move || usage_now(&app.state::<AppState>(), focus))
        .await
        .map_err(|e| e.to_string())
}

fn usage_now(state: &AppState, focus: Option<FocusArg>) -> UsageResponse {
    let settings = state.typed_settings();
    let dir = agent_dir(state);
    if settings.agent.live_quota {
        for kind in [AgentKind::Claude, AgentKind::Codex] {
            let active = focus.as_ref().is_some_and(|f| f.kind == kind);
            maybe_refresh(state, kind, active, false);
        }
    }
    let (live, live_errors, refreshing) = {
        let mut cache = state.agent.live.lock().unwrap();
        let now = now_epoch();
        if now - cache.pruned_at > 3600 {
            cache.pruned_at = now;
            mooncli_agent::spool::prune(&dir, SPOOL_KEEP, now);
        }
        if settings.agent.live_quota {
            let live: Vec<AccountUsage> =
                [&cache.claude, &cache.codex].iter().filter_map(|s| s.account.clone()).collect();
            let errors = LiveErrors { claude: cache.claude.last_error.clone(), codex: cache.codex.last_error.clone() };
            let refreshing = [(AgentKind::Claude, &cache.claude), (AgentKind::Codex, &cache.codex)]
                .into_iter()
                .filter(|(_, s)| s.in_flight)
                .map(|(k, _)| k)
                .collect();
            (live, errors, refreshing)
        } else {
            (Vec::new(), LiveErrors::default(), Vec::new())
        }
    };
    // 标签页记的开始时刻和 Agent 真正写下第一条记录之间可能差一点，放宽几秒
    let focus = focus.map(|f| Focus { kind: f.kind, tab_id: f.tab_id, cwd: f.cwd, since: f.since_ms / 1000 - 5 });
    let report = mooncli_agent::collect(&CollectOptions::defaults(dir), &live, focus.as_ref());
    UsageResponse { accounts: report.accounts, current: report.current, live_errors, refreshing }
}

/// 立刻重查两个 Agent 的额度（卡片上的「刷新」）。两个一起查，都查完才返回；有失败时返回原因。
#[tauri::command]
pub async fn agent_refresh_live(state: State<'_, AppState>) -> Result<(), String> {
    if !state.typed_settings().agent.live_quota {
        return Err("主动查询额度没有开启（设置 → Agent）".into());
    }
    let handles: Vec<_> = [AgentKind::Claude, AgentKind::Codex]
        .into_iter()
        .filter_map(|kind| maybe_refresh(&state, kind, true, true).map(|h| (kind, h)))
        .collect();
    let mut errors = Vec::new();
    for (kind, h) in handles {
        let result = tauri::async_runtime::spawn_blocking(move || h.join()).await;
        if let Ok(Ok(Err(e))) = result {
            errors.push(format!("{}：{e}", if kind == AgentKind::Claude { "Claude Code" } else { "Codex" }));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("；")) }
}

/// 该查了就在后台线程里查一次，返回线程句柄；不该查（没到时间、正在查）返回 None。
/// `force` 是用户点了刷新：不看间隔，只要没在查就查。
fn maybe_refresh(
    state: &AppState,
    kind: AgentKind,
    active: bool,
    force: bool,
) -> Option<std::thread::JoinHandle<Result<(), String>>> {
    let now = now_epoch();
    let dir = agent_dir(state);
    let activity = mooncli_agent::spool::latest_mtime(&dir, activity_prefixes(kind));
    {
        let mut cache = state.agent.live.lock().unwrap();
        let slot = cache.slot(kind);
        if slot.in_flight {
            return None;
        }
        let interval = if slot.last_error.is_some() {
            LIVE_RETRY_STEPS.get(slot.failures.saturating_sub(1)).copied().unwrap_or(LIVE_RETRY_AFTER_ERROR)
        } else if active {
            if kind == AgentKind::Claude { LIVE_INTERVAL_ACTIVE_CLAUDE } else { LIVE_INTERVAL_ACTIVE }
        } else if kind == AgentKind::Claude {
            LIVE_INTERVAL_CLAUDE
        } else {
            LIVE_INTERVAL_CODEX
        };
        let since = now - slot.last_try;
        let gap = if kind == AgentKind::Claude { LIVE_MIN_GAP_CLAUDE } else { LIVE_MIN_GAP };
        let fresh_activity = activity > slot.seen_activity && since >= gap;
        if !force && since < interval && !fresh_activity {
            return None;
        }
        slot.in_flight = true;
        slot.last_try = now;
        slot.seen_activity = activity;
    }
    let settings = state.typed_settings().agent;
    let agent = state.agent.clone();
    Some(std::thread::spawn(move || {
        let result = match kind {
            AgentKind::Codex => mooncli_agent::codex_live::find_codex_exe(settings.codex_path.as_deref())
                .ok_or_else(|| "没有找到 codex，可以在设置里指定位置".to_string())
                .and_then(|exe| mooncli_agent::codex_live::fetch(&exe, Duration::from_secs(20))),
            AgentKind::Claude => mooncli_agent::claude_usage::find_claude_exe(settings.claude_path.as_deref())
                .ok_or_else(|| "没有找到 claude，可以在设置里指定位置".to_string())
                .and_then(|exe| mooncli_agent::claude_usage::fetch(&exe, &dir.join("probe"), Duration::from_secs(40))),
        };
        store_live(&agent, kind, result)
    }))
}

fn store_live(agent: &AgentState, kind: AgentKind, result: Result<AccountUsage, String>) -> Result<(), String> {
    let mut cache = agent.live.lock().unwrap();
    let slot = cache.slot(kind);
    slot.in_flight = false;
    match result {
        Ok(a) => {
            slot.account = Some(a);
            slot.last_error = None;
            slot.failures = 0;
            Ok(())
        }
        // 上次查到的留着（卡片会标出那是几点的数据），只记下这次的失败原因
        Err(e) => {
            slot.last_error = Some(e.clone());
            slot.failures += 1;
            Err(e)
        }
    }
}

// ---- 完成提醒 ------------------------------------------------------------------

/// 启动时准备好通知：登记通知身份（名字 + 图标），把提醒偏好写给 bridge，
/// 再起一个线程收 bridge 交来的提醒（Agent 跑在 mooncli 标签页里时），交给界面决定弹不弹。
pub fn start_alerts(app: tauri::AppHandle) {
    let state = app.state::<AppState>();
    let dir = agent_dir(&state);
    let _ = std::fs::create_dir_all(&dir);
    let icon = dir.join("mooncli.png");
    let png: &[u8] = include_bytes!("../icons/128x128.png");
    if std::fs::metadata(&icon).map(|m| m.len() != png.len() as u64).unwrap_or(true) {
        let _ = std::fs::write(&icon, png);
    }
    if let Err(e) = notify::register_app_id(Some(&icon)) {
        crate::logs::append(&state.data.data_dir, "error", &e);
    }
    sync_notify_prefs(&state);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let state = app.state::<AppState>();
            let alerts = notify::take_alerts(&agent_dir(&state));
            if alerts.is_empty() || !state.typed_settings().agent.notify.enabled {
                continue;
            }
            for alert in alerts {
                let _ = app.emit_to("main", "agent-alert", alert);
            }
        }
    });
}

/// 把提醒偏好写进 bridge-config：bridge 在 mooncli 外面运行时照着它决定弹不弹。
pub fn sync_notify_prefs(state: &AppState) {
    let prefs = state.typed_settings().agent.notify;
    let mut cfg = read_bridge_config(state);
    let want = json!({ "enabled": prefs.enabled, "quietWhenFocused": prefs.quiet_when_focused });
    if cfg.get("notify") != Some(&want) {
        cfg["notify"] = want;
        let _ = write_bridge_config(state, &cfg);
    }
}

/// 弹一条系统通知。带 tab_id 时，点通知会把 mooncli 拉到前面并切到那个标签页。
#[tauri::command]
pub fn notify_toast(app: tauri::AppHandle, title: String, body: String, tab_id: Option<String>) -> Result<(), String> {
    let handle = app.clone();
    let on_click: Box<dyn Fn() + Send> = Box::new(move || {
        if let Some(w) = handle.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        if let Some(id) = &tab_id {
            let _ = handle.emit_to("main", "toast-activated", id.clone());
        }
    });
    notify::show_toast(&title, &body, Some(on_click))
}

// ---- bridge 与一键接入 -----------------------------------------------------------

fn claude_integrated() -> bool {
    read_text(&claude_settings_path())
        .ok()
        .flatten()
        .and_then(|t| parse_claude_settings(Some(&t)).ok())
        .is_some_and(|v| integrate::claude_is_integrated(&v))
}

/// mooncli-bridge 的完整路径（和主程序放在一起发布）。一键接入时写进 Agent 的配置。
#[tauri::command]
pub fn agent_bridge_path() -> String {
    bridge_path()
}

fn bridge_path() -> String {
    let name = if cfg!(windows) { "mooncli-bridge.exe" } else { "mooncli-bridge" };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| name.to_string())
}

// ---- 一键接入 ------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub claude: bool,
    pub codex: bool,
    pub claude_settings_path: String,
    pub codex_config_path: String,
    /// 找到的 codex 可执行文件（实时额度要用）
    pub codex_exe: Option<String>,
    pub bridge_exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationOutcome {
    pub changes: Vec<String>,
}

fn codex_config_path() -> PathBuf {
    mooncli_agent::codex::codex_homes().into_iter().next().unwrap_or_default().join("config.toml")
}

fn claude_settings_path() -> PathBuf {
    mooncli_agent::claude::claude_config_dir().join("settings.json")
}

fn read_text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取 {} 失败：{e}", path.display())),
    }
}

fn parse_claude_settings(text: Option<&str>) -> Result<Value, String> {
    match text.map(str::trim) {
        None | Some("") => Ok(Value::Null),
        Some(t) => serde_json::from_str(t).map_err(|e| format!("Claude Code 的 settings.json 不是合法的 JSON：{e}")),
    }
}

/// bridge-config.json：接入时保存的用户原设置（原 statusLine、原 notify、原文件内容），
/// bridge 运行时转调，断开时恢复。
fn bridge_config_path(state: &AppState) -> PathBuf {
    agent_dir(state).join("bridge-config.json")
}

fn read_bridge_config(state: &AppState) -> Value {
    std::fs::read_to_string(bridge_config_path(state))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn write_bridge_config(state: &AppState, cfg: &Value) -> Result<(), String> {
    let dir = agent_dir(state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败：{e}"))?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    files::write_atomic(&bridge_config_path(state), text.as_bytes()).map_err(|e| format!("写 bridge-config 失败：{e}"))
}

fn previous_notify(cfg: &Value) -> Vec<String> {
    cfg.get("previousCodexNotify")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// 改用户配置文件前先备份：<原文件>.mooncli-backup-<时间戳>，和原文件放一起。
fn backup(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let backup = path.with_file_name(format!(
        "{}.mooncli-backup-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("config"),
        now_epoch()
    ));
    std::fs::copy(path, &backup).map(|_| ()).map_err(|e| format!("备份 {} 失败：{e}", path.display()))
}

/// Claude Code 自己写 settings.json 用两格缩进，这里保持一致。
fn claude_json_text(v: &Value) -> String {
    let mut text = serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}

#[tauri::command]
pub fn agent_integration_status() -> IntegrationStatus {
    let codex = read_text(&codex_config_path()).ok().flatten().is_some_and(|t| integrate::codex_is_integrated(&t));
    IntegrationStatus {
        claude: claude_integrated(),
        codex,
        claude_settings_path: claude_settings_path().display().to_string(),
        codex_config_path: codex_config_path().display().to_string(),
        codex_exe: mooncli_agent::codex_live::find_codex_exe(None).map(|p| p.display().to_string()),
        bridge_exists: Path::new(&bridge_path()).is_file(),
    }
}

/// 要写的结果：新文件内容（None 表示删掉文件）+ 更新后的 bridge-config + 变更清单
struct Plan {
    path: PathBuf,
    text: Option<String>,
    cfg: Value,
    changes: Vec<String>,
}

fn plan(state: &AppState, target: &str, action: &str) -> Result<Plan, String> {
    let bridge = bridge_path();
    let mut cfg = read_bridge_config(state);
    match (target, action) {
        ("claude", "integrate") => {
            let path = claude_settings_path();
            let original = read_text(&path)?;
            let current = parse_claude_settings(original.as_deref())?;
            if integrate::claude_is_integrated(&current) {
                return Ok(Plan { path, text: original, cfg, changes: vec!["已经接入，不需要改动".into()] });
            }
            let out = integrate::claude_integrate(&current, &bridge)?;
            let text = claude_json_text(&out.settings);
            cfg["previousClaudeStatusLine"] = out.previous_statusline.clone().unwrap_or(Value::Null);
            cfg["claudeCreatedEvents"] = json!(out.created_events);
            cfg["claudeCreatedHooks"] = json!(out.created_hooks);
            // 断开时文件若没被别人改过，就把原文逐字节放回去
            cfg["claudeOriginalText"] = original.map(Value::String).unwrap_or(Value::Null);
            cfg["claudeIntegratedText"] = json!(text);
            Ok(Plan { path, text: Some(text), cfg, changes: out.changes })
        }
        ("claude", "unintegrate") => {
            let path = claude_settings_path();
            let text = read_text(&path)?;
            let current = parse_claude_settings(text.as_deref())?;
            let untouched = cfg.get("claudeIntegratedText").and_then(Value::as_str) == text.as_deref();
            let (new_text, changes) = if untouched && cfg.get("claudeIntegratedText").is_some() {
                let original = cfg.get("claudeOriginalText").and_then(Value::as_str).map(str::to_string);
                let msg = if original.is_some() {
                    "settings.json 恢复成接入前的原文"
                } else {
                    "删除接入时新建的 settings.json"
                };
                (original, vec![msg.to_string()])
            } else {
                let created: Vec<String> = cfg
                    .get("claudeCreatedEvents")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let prev = cfg.get("previousClaudeStatusLine").filter(|v| !v.is_null());
                let created_hooks = cfg.get("claudeCreatedHooks").and_then(Value::as_bool).unwrap_or(false);
                let out = integrate::claude_unintegrate(&current, prev, &created, created_hooks)?;
                (Some(claude_json_text(&out.settings)), out.changes)
            };
            if let Some(obj) = cfg.as_object_mut() {
                for k in [
                    "previousClaudeStatusLine",
                    "claudeCreatedEvents",
                    "claudeCreatedHooks",
                    "claudeOriginalText",
                    "claudeIntegratedText",
                ] {
                    obj.remove(k);
                }
            }
            Ok(Plan { path, text: new_text, cfg, changes })
        }
        ("codex", "integrate") => {
            let path = codex_config_path();
            let original = read_text(&path)?.unwrap_or_default();
            let out = integrate::codex_integrate(&original, &bridge);
            if let Some(prev) = &out.previous_notify {
                cfg["previousCodexNotify"] = json!(prev);
            }
            if let Some(line) = &out.previous_line {
                cfg["previousCodexNotifyLine"] = json!(line);
            }
            Ok(Plan { path, text: Some(out.text), cfg, changes: out.changes })
        }
        ("codex", "unintegrate") => {
            let path = codex_config_path();
            let prev = previous_notify(&cfg);
            let line = cfg.get("previousCodexNotifyLine").and_then(Value::as_str).map(str::to_string);
            let out = integrate::codex_unintegrate(
                &read_text(&path)?.unwrap_or_default(),
                line.as_deref(),
                if prev.is_empty() { None } else { Some(&prev) },
            );
            if let Some(obj) = cfg.as_object_mut() {
                obj.remove("previousCodexNotify");
                obj.remove("previousCodexNotifyLine");
            }
            Ok(Plan { path, text: Some(out.text), cfg, changes: out.changes })
        }
        _ => Err("未知的接入动作".into()),
    }
}

/// 预览接入/断开会改什么（不落盘）。target: "claude" | "codex"；action: "integrate" | "unintegrate"。
#[tauri::command]
pub fn agent_integration_preview(
    state: State<'_, AppState>,
    target: String,
    action: String,
) -> Result<IntegrationOutcome, String> {
    plan(&state, &target, &action).map(|p| IntegrationOutcome { changes: p.changes })
}

/// 执行接入/断开：重新读取最新文件再合并（避免预览后文件被改过），改前备份。
#[tauri::command]
pub fn agent_integration_apply(
    state: State<'_, AppState>,
    target: String,
    action: String,
) -> Result<IntegrationOutcome, String> {
    let p = plan(&state, &target, &action)?;
    let current = read_text(&p.path)?;
    if current == p.text {
        write_bridge_config(&state, &p.cfg)?;
        return Ok(IntegrationOutcome { changes: p.changes });
    }
    backup(&p.path)?;
    write_bridge_config(&state, &p.cfg)?;
    match &p.text {
        Some(text) => {
            if let Some(dir) = p.path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("创建 {} 失败：{e}", dir.display()))?;
            }
            files::write_atomic(&p.path, text.as_bytes()).map_err(|e| format!("写 {} 失败：{e}", p.path.display()))?
        }
        None => std::fs::remove_file(&p.path).map_err(|e| format!("删除 {} 失败：{e}", p.path.display()))?,
    }
    Ok(IntegrationOutcome { changes: p.changes })
}

/// 独立置顶小窗：只显示额度卡片（docs/07 第 5 节）。
/// 必须是 async 命令：Windows 上在同步命令里建窗口会让新窗口的 WebView 卡死（白屏）。
#[tauri::command]
pub async fn agent_window_toggle(app: tauri::AppHandle, open: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("agent") {
        if open {
            let _ = w.show();
            let _ = w.set_focus();
        } else {
            let _ = w.close();
        }
        return Ok(());
    }
    if !open {
        return Ok(());
    }
    // 用初始化脚本告诉界面这是小窗（WebviewUrl::App 不保留查询参数）
    tauri::WebviewWindowBuilder::new(&app, "agent", tauri::WebviewUrl::App("index.html".into()))
        .initialization_script("window.__MOONCLI_VIEW = 'agent';")
        .title("mooncli · Agent 额度")
        .inner_size(320.0, 230.0)
        .min_inner_size(260.0, 140.0)
        .always_on_top(true)
        .decorations(false)
        .skip_taskbar(true)
        .resizable(true)
        .build()
        .map(|_| ())
        .map_err(|e| format!("打开独立小窗失败：{e}"))
}
