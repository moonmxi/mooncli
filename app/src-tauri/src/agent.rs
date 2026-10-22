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

/// 实时额度多久查一次（秒）。app-server 每次要起一个 Codex 进程（约 3–5 秒 CPU），不宜太勤；
/// 额度窗口按小时计，两分钟足够新。
const LIVE_INTERVAL: i64 = 120;
/// 连续失败后隔多久再试（秒）：没装 Codex、没登录时别反复起进程
const LIVE_RETRY_AFTER_ERROR: i64 = 600;
/// spool 文件多久没更新就清掉（秒）
const SPOOL_KEEP: i64 = 7 * 86_400;

#[derive(Default)]
pub struct AgentState {
    live: Mutex<LiveCache>,
}

#[derive(Default)]
struct LiveCache {
    accounts: Vec<AccountUsage>,
    /// 上次开始查询的时刻（epoch 秒）
    last_try: i64,
    last_error: Option<String>,
    in_flight: bool,
    pruned_at: i64,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    pub accounts: Vec<AccountUsage>,
    pub current: Option<CurrentSession>,
    /// 实时额度最近一次查询失败的原因（成功或没开时为空）
    pub live_error: Option<String>,
    /// Claude Code 接入了 statusLine 没有（没接入就拿不到它的额度，卡片上给出提示）
    pub claude_integrated: bool,
}

/// 采集两个 Agent 的额度，以及当前标签页里正在跑的那个会话的上下文。
/// 顺手在后台刷新 Codex 实时额度（到点了才刷），本次先用缓存，下一轮轮询就能拿到新的。
#[tauri::command]
pub fn agent_usage(state: State<'_, AppState>, focus: Option<FocusArg>) -> UsageResponse {
    let settings = state.typed_settings();
    let dir = agent_dir(&state);
    if settings.agent.live_quota {
        maybe_refresh_live(&state, settings.agent.codex_path.clone());
    }
    let (live, live_error) = {
        let mut cache = state.agent.live.lock().unwrap();
        let now = now_epoch();
        if now - cache.pruned_at > 3600 {
            cache.pruned_at = now;
            mooncli_agent::spool::prune(&dir, SPOOL_KEEP, now);
        }
        if settings.agent.live_quota { (cache.accounts.clone(), cache.last_error.clone()) } else { (Vec::new(), None) }
    };
    // 标签页记的开始时刻和 Agent 真正写下第一条记录之间可能差一点，放宽几秒
    let focus = focus.map(|f| Focus { kind: f.kind, tab_id: f.tab_id, cwd: f.cwd, since: f.since_ms / 1000 - 5 });
    let report = mooncli_agent::collect(&CollectOptions::defaults(dir), &live, focus.as_ref());
    UsageResponse {
        accounts: report.accounts,
        current: report.current,
        live_error,
        claude_integrated: claude_integrated(),
    }
}

/// 立刻重新查一次实时额度（卡片上的「刷新」）。查完才返回。
#[tauri::command]
pub async fn agent_refresh_live(state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.typed_settings();
    if !settings.agent.live_quota {
        return Err("实时额度没有开启".into());
    }
    let exe = mooncli_agent::codex_live::find_codex_exe(settings.agent.codex_path.as_deref())
        .ok_or("没有找到 codex，可以在设置里指定位置")?;
    {
        let mut cache = state.agent.live.lock().unwrap();
        if cache.in_flight {
            return Ok(());
        }
        cache.in_flight = true;
        cache.last_try = now_epoch();
    }
    let result =
        tauri::async_runtime::spawn_blocking(move || mooncli_agent::codex_live::fetch(&exe, Duration::from_secs(20)))
            .await
            .map_err(|e| e.to_string())?;
    store_live(&state.agent, result)
}

fn store_live(agent: &AgentState, result: Result<AccountUsage, String>) -> Result<(), String> {
    let mut cache = agent.live.lock().unwrap();
    cache.in_flight = false;
    match result {
        Ok(a) => {
            cache.accounts.retain(|x| x.kind != a.kind);
            cache.accounts.push(a);
            cache.last_error = None;
            Ok(())
        }
        Err(e) => {
            cache.last_error = Some(e.clone());
            Err(e)
        }
    }
}

fn maybe_refresh_live(state: &AppState, codex_path: Option<String>) {
    let now = now_epoch();
    {
        let mut cache = state.agent.live.lock().unwrap();
        let wait = if cache.last_error.is_some() { LIVE_RETRY_AFTER_ERROR } else { LIVE_INTERVAL };
        if cache.in_flight || now - cache.last_try < wait {
            return;
        }
        cache.in_flight = true;
        cache.last_try = now;
    }
    let Some(exe) = mooncli_agent::codex_live::find_codex_exe(codex_path.as_deref()) else {
        let _ = store_live(&state.agent, Err("没有找到 codex".into()));
        return;
    };
    let agent = state.agent.clone();
    std::thread::spawn(move || {
        let result = mooncli_agent::codex_live::fetch(&exe, Duration::from_secs(20));
        let _ = store_live(&agent, result);
    });
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
