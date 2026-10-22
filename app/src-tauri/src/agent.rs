//! Agent 用量小窗的主进程侧（docs/07）：采集命令 + bridge 位置 + 一键接入/断开。
//! 采集与配置合并的纯逻辑在 mooncli-agent 里，这里负责路径、备份和落盘。

use mooncli_agent::integrate;
use mooncli_agent::{CollectOptions, Sources, UsageReport};
use serde::Serialize;
use tauri::State;

use crate::files;
use crate::AppState;

/// mooncli 的 agent 数据目录：data/agent（bridge 的 spool 与 bridge-config 在这里）。
pub fn agent_dir(state: &AppState) -> std::path::PathBuf {
    state.data.data_dir.join("agent")
}

/// 采集当前用量。功能没开时返回空报告，界面据此不显示小窗。
#[tauri::command]
pub fn agent_usage(state: State<'_, AppState>) -> UsageReport {
    let settings = state.typed_settings();
    if !settings.agent.enabled {
        return UsageReport::default();
    }
    let dir = agent_dir(&state);
    let mut opts = CollectOptions::defaults(dir);
    opts.sources = Sources {
        files: settings.agent.read_session_files,
        statusline: settings.agent.receive_statusline,
        hooks: settings.agent.receive_hooks,
    };
    mooncli_agent::collect_all(&opts)
}

/// mooncli-bridge 的完整路径（和主程序放在一起发布）。一键接入时写进 Claude Code / Codex 的配置。
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

// ---- 一键接入 ----------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub claude: bool,
    pub codex: bool,
    pub claude_settings_path: String,
    pub codex_config_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationOutcome {
    pub changes: Vec<String>,
}

fn claude_settings_path() -> std::path::PathBuf {
    mooncli_agent::claude::claude_config_dir().join("settings.json")
}

fn codex_config_path() -> std::path::PathBuf {
    mooncli_agent::codex::codex_homes().into_iter().next().unwrap_or_default().join("config.toml")
}

fn read_claude_settings() -> Result<serde_json::Value, String> {
    let path = claude_settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("解析 {} 失败：{e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(e) => Err(format!("读取 {} 失败：{e}", path.display())),
    }
}

fn read_codex_config() -> Result<String, String> {
    let path = codex_config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("读取 {} 失败：{e}", path.display())),
    }
}

/// bridge-config.json：接入时保存的用户原命令，bridge 运行时转调用。
fn bridge_config_path(state: &AppState) -> std::path::PathBuf {
    agent_dir(state).join("bridge-config.json")
}

fn read_bridge_config(state: &AppState) -> serde_json::Value {
    std::fs::read_to_string(bridge_config_path(state))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::json!({}))
}

fn write_bridge_config(state: &AppState, cfg: &serde_json::Value) -> Result<(), String> {
    let dir = agent_dir(state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败：{e}"))?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    files::write_atomic(&bridge_config_path(state), text.as_bytes()).map_err(|e| format!("写 bridge-config 失败：{e}"))
}

/// 改用户配置文件前先备份：<原文件>.mooncli-backup-<时间戳>，和原文件放一起。
fn backup(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = path.with_file_name(format!(
        "{}.mooncli-backup-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("config"),
        stamp
    ));
    std::fs::copy(path, &backup)
        .map(|_| ())
        .map_err(|e| format!("备份 {} 失败：{e}", path.display()))
}

#[tauri::command]
pub fn agent_integration_status() -> Result<IntegrationStatus, String> {
    let claude = read_claude_settings().map(|s| integrate::claude_is_integrated(&s)).unwrap_or(false);
    let codex = read_codex_config().map(|t| integrate::codex_is_integrated(&t)).unwrap_or(false);
    Ok(IntegrationStatus {
        claude,
        codex,
        claude_settings_path: claude_settings_path().display().to_string(),
        codex_config_path: codex_config_path().display().to_string(),
    })
}

/// 预览接入/断开会改什么（不落盘）。target: "claude" | "codex"，action: "integrate" | "unintegrate"。
#[tauri::command]
pub fn agent_integration_preview(state: State<'_, AppState>, target: String, action: String) -> Result<IntegrationOutcome, String> {
    let bridge = bridge_path();
    let cfg = read_bridge_config(&state);
    match (target.as_str(), action.as_str()) {
        ("claude", "integrate") => {
            let out = integrate::claude_integrate(&read_claude_settings()?, &bridge)?;
            Ok(IntegrationOutcome { changes: out.changes })
        }
        ("claude", "unintegrate") => {
            let prev = cfg.get("previousStatusLine").and_then(|v| v.as_str());
            let out = integrate::claude_unintegrate(&read_claude_settings()?, prev)?;
            Ok(IntegrationOutcome { changes: out.changes })
        }
        ("codex", "integrate") => {
            let out = integrate::codex_integrate(&read_codex_config()?, &bridge);
            Ok(IntegrationOutcome { changes: out.changes })
        }
        ("codex", "unintegrate") => {
            let prev: Vec<String> = cfg
                .get("previousCodexNotify")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let out = integrate::codex_unintegrate(&read_codex_config()?, if prev.is_empty() { None } else { Some(&prev) });
            Ok(IntegrationOutcome { changes: out.changes })
        }
        _ => Err("未知的接入目标或动作".into()),
    }
}

/// 执行接入/断开：重新读取最新文件再合并（避免预览后文件被改过），改前备份。
#[tauri::command]
pub fn agent_integration_apply(state: State<'_, AppState>, target: String, action: String) -> Result<IntegrationOutcome, String> {
    let bridge = bridge_path();
    let mut cfg = read_bridge_config(&state);
    if !cfg.is_object() {
        cfg = serde_json::json!({});
    }
    match (target.as_str(), action.as_str()) {
        ("claude", "integrate") => {
            let path = claude_settings_path();
            let out = integrate::claude_integrate(&read_claude_settings()?, &bridge)?;
            backup(&path)?;
            if let Some(prev) = &out.previous_statusline {
                cfg["previousStatusLine"] = serde_json::Value::String(prev.clone());
                write_bridge_config(&state, &cfg)?;
            }
            let text = serde_json::to_string_pretty(&out.settings).map_err(|e| e.to_string())?;
            files::write_atomic(&path, format!("{text}\n").as_bytes()).map_err(|e| format!("写 settings.json 失败：{e}"))?;
            Ok(IntegrationOutcome { changes: out.changes })
        }
        ("claude", "unintegrate") => {
            let path = claude_settings_path();
            let prev = cfg.get("previousStatusLine").and_then(|v| v.as_str()).map(|s| s.to_string());
            let out = integrate::claude_unintegrate(&read_claude_settings()?, prev.as_deref())?;
            backup(&path)?;
            if let Some(obj) = cfg.as_object_mut() {
                obj.remove("previousStatusLine");
            }
            write_bridge_config(&state, &cfg)?;
            let text = serde_json::to_string_pretty(&out.settings).map_err(|e| e.to_string())?;
            files::write_atomic(&path, format!("{text}\n").as_bytes()).map_err(|e| format!("写 settings.json 失败：{e}"))?;
            Ok(IntegrationOutcome { changes: out.changes })
        }
        ("codex", "integrate") => {
            let path = codex_config_path();
            let out = integrate::codex_integrate(&read_codex_config()?, &bridge);
            backup(&path)?;
            if let Some(prev) = &out.previous_notify {
                cfg["previousCodexNotify"] = serde_json::Value::Array(prev.iter().map(|s| serde_json::Value::String(s.clone())).collect());
                write_bridge_config(&state, &cfg)?;
            }
            files::write_atomic(&path, out.text.as_bytes()).map_err(|e| format!("写 config.toml 失败：{e}"))?;
            Ok(IntegrationOutcome { changes: out.changes })
        }
        ("codex", "unintegrate") => {
            let path = codex_config_path();
            let prev: Vec<String> = cfg
                .get("previousCodexNotify")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let out = integrate::codex_unintegrate(&read_codex_config()?, if prev.is_empty() { None } else { Some(&prev) });
            backup(&path)?;
            if let Some(obj) = cfg.as_object_mut() {
                obj.remove("previousCodexNotify");
            }
            write_bridge_config(&state, &cfg)?;
            files::write_atomic(&path, out.text.as_bytes()).map_err(|e| format!("写 config.toml 失败：{e}"))?;
            Ok(IntegrationOutcome { changes: out.changes })
        }
        _ => Err("未知的接入目标或动作".into()),
    }
}
