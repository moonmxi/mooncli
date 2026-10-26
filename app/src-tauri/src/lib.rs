//! mooncli 主程序：窗口、会话管理、设置、剪贴板、历史库。界面在 app/src/。

mod agent;
mod conpty;
mod data;
mod files;
mod fonts;
mod logs;
mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mooncli_config::Settings;
use mooncli_history::{Filter, HistoryService, Hit, HostCount, Page, Query, Run, Store, Summary};
use mooncli_pty::Session;
use mooncli_pty::shell::{
    LaunchRequest, ShellSpec, find_shell, find_windows_powershell, find_winget, install_pwsh_launch, launch,
};
use serde::Serialize;
use session::{UiMsg, UiSink};
use tauri::ipc::Channel;
use tauri::{Manager, State};

struct SessionEntry {
    session: Arc<Session>,
    sink: Arc<UiSink>,
}

struct AppState {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    shell: Mutex<Option<ShellSpec>>,
    history: Arc<Mutex<Option<HistoryService>>>,
    /// 界面保存的原样设置（JSON）。Rust 端只读其中自己用得到的字段。
    settings: Mutex<serde_json::Value>,
    settings_path: PathBuf,
    backgrounds_dir: PathBuf,
    history_path: PathBuf,
    /// 剪贴板对象要一直留着：Linux 上它一释放，写进去的内容就没了
    clipboard: Mutex<Option<arboard::Clipboard>>,
    bundled_conpty: bool,
    os_version: String,
    data: data::DataPaths,
    agent: Arc<agent::AgentState>,
}

impl AppState {
    fn typed_settings(&self) -> Settings {
        serde_json::from_value(self.settings.lock().unwrap().clone()).unwrap_or_default()
    }
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---- 会话 --------------------------------------------------------------------

/// 找不到 pwsh 7 时 session_create 返回的错误以它开头，界面据此显示安装引导（docs/02 §3.1）
const NO_PWSH: &str = "NO_PWSH";

/// `launch`：不填是平常的 pwsh 7；"windowsPowerShell" 临时用系统自带的 5.1；"installPwsh" 用 winget 安装 pwsh 7
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn session_create(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
    launch_mode: Option<String>,
    channel: Channel<UiMsg>,
) -> CmdResult<()> {
    let settings = state.typed_settings();
    let cwd = cwd.filter(|c| std::path::Path::new(c).is_dir()).or_else(dirs_home);
    let shell = match launch_mode.as_deref() {
        Some("windowsPowerShell") | Some("installPwsh") => {
            find_windows_powershell().ok_or("这台电脑上也没有找到 Windows PowerShell（powershell.exe）。")?
        }
        _ => state
            .shell
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| format!("{NO_PWSH}: 没有找到 PowerShell 7（pwsh.exe）"))?,
    };
    let agent_dir_string =
        if settings.agent.enabled { agent::agent_dir(&state).display().to_string() } else { String::new() };
    let opts = if launch_mode.as_deref() == Some("installPwsh") {
        install_pwsh_launch(&shell, cwd, cols, rows)
    } else {
        launch(LaunchRequest {
            shell: &shell,
            extra_args: &settings.shell.args,
            integration: settings.shell.integration,
            adapt_colors: settings.shell.adapt_colors,
            session_id: &id,
            agent_dir: if settings.agent.enabled { Some(agent_dir_string.as_str()) } else { None },
            cwd,
            cols,
            rows,
            app_version: env!("CARGO_PKG_VERSION"),
        })
    };
    let sink = Arc::new(UiSink::new(id.clone(), channel, state.history.clone()));
    sink.set_connect_commands(settings.remote.connect_commands.clone());
    let session = Session::spawn(opts, sink.clone()).map_err(err)?;
    if let Some(pid) = session.pid() {
        UiSink::watch_processes(Arc::downgrade(&sink), pid);
    }
    // 同一个 id 再创建（比如界面重新加载）时，旧会话随 drop 结束
    state.sessions.lock().unwrap().insert(id, SessionEntry { session: Arc::new(session), sink });
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellStatus {
    /// 找到的 pwsh 7（或设置里指定的 shell）
    pwsh: Option<String>,
    /// 设置里指定了路径，但那里没有文件
    configured_missing: bool,
    windows_powershell: Option<String>,
    winget: bool,
}

/// 重新找一遍 shell。刚装好 pwsh 时调用，找到了以后新开的标签页就用它
#[tauri::command]
fn shell_status(state: State<'_, AppState>) -> ShellStatus {
    let configured = state.typed_settings().shell.path;
    let found = find_shell(configured.as_deref());
    *state.shell.lock().unwrap() = found.clone();
    ShellStatus {
        pwsh: found.map(|s| s.program.display().to_string()),
        configured_missing: configured.is_some_and(|p| !std::path::Path::new(&p).is_file()),
        windows_powershell: find_windows_powershell().map(|s| s.program.display().to_string()),
        winget: find_winget().is_some(),
    }
}

/// 用系统浏览器打开网页。只放行 https，免得被拿来打开本机文件
#[tauri::command]
fn open_url(url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("只能打开 https 网址".into());
    }
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .creation_flags(0x0800_0000)
            .spawn()
    };
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(err)
}

fn dirs_home() -> Option<String> {
    std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok()
}

fn with_session<T>(state: &State<'_, AppState>, id: &str, f: impl FnOnce(&Session) -> T) -> CmdResult<T> {
    let s =
        state.sessions.lock().unwrap().get(id).map(|e| e.session.clone()).ok_or_else(|| format!("会话 {id} 不存在"))?;
    Ok(f(&s))
}

#[tauri::command]
fn session_write(state: State<'_, AppState>, id: String, data: String) -> CmdResult<()> {
    with_session(&state, &id, |s| s.write_chunked(data.as_bytes()))?.map_err(err)
}

#[tauri::command]
fn session_resize(state: State<'_, AppState>, id: String, cols: u16, rows: u16) -> CmdResult<()> {
    with_session(&state, &id, |s| s.resize(cols, rows))?.map_err(err)
}

#[tauri::command]
fn session_ack(state: State<'_, AppState>, id: String, bytes: usize) -> CmdResult<()> {
    with_session(&state, &id, |s| s.ack(bytes))
}

#[tauri::command]
fn session_stats(state: State<'_, AppState>, id: String) -> CmdResult<session::SessionStats> {
    state.sessions.lock().unwrap().get(&id).map(|e| e.sink.stats()).ok_or_else(|| format!("会话 {id} 不存在"))
}

/// 界面读出的远程命令文字（见 session.rs 的 RemoteCmd）
#[tauri::command]
fn session_remote_command(state: State<'_, AppState>, id: String, seq: u64, text: String) -> CmdResult<()> {
    let sink =
        state.sessions.lock().unwrap().get(&id).map(|e| e.sink.clone()).ok_or_else(|| format!("会话 {id} 不存在"))?;
    sink.remote_command_text(seq, &text);
    Ok(())
}

#[tauri::command]
fn session_close(state: State<'_, AppState>, id: String) {
    state.sessions.lock().unwrap().remove(&id);
}

// ---- 历史 --------------------------------------------------------------------
//
// 历史命令全部是异步命令：在后台线程里等历史库，界面线程一刻都不等（docs/04 §3"性能"）。
// 搜索交给 HistoryService 的搜索线程，同一路只算最新的查询。

impl AppState {
    /// 历史库服务（还没打开完、或者关了历史功能时为 None）。克隆出来就放开锁，不会挡住终端写历史。
    fn history_service(&self) -> Option<HistoryService> {
        self.history.lock().unwrap().clone()
    }
}

/// 在后台线程里用历史库做一件事
async fn on_history<T: Send + 'static>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&HistoryService) -> mooncli_history::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let svc = app.state::<AppState>().history_service().ok_or("历史库还在加载，请稍等")?;
    tauri::async_runtime::spawn_blocking(move || f(&svc).map_err(err)).await.map_err(err)?
}

/// 搜一页。`lane` 区分是谁在搜（历史面板、补全面板、命令面板），同一路只算最新的查询；
/// 被更新的查询顶掉时返回错误 "superseded"，界面忽略即可。
#[tauri::command]
async fn history_page(
    app: tauri::AppHandle,
    query: Query,
    lane: String,
    offset: usize,
    limit: usize,
) -> CmdResult<Page> {
    let svc = app.state::<AppState>().history_service().ok_or("历史库还在加载，请稍等")?;
    let rx = svc.search(&lane, query, offset, limit.clamp(1, 1000));
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(err)?).await.map_err(err)?
}

/// 简单搜索（补全面板、命令面板）：取前 `query.limit` 条
#[tauri::command]
async fn history_search(app: tauri::AppHandle, query: Query, lane: Option<String>) -> CmdResult<Vec<Hit>> {
    let limit = query.limit;
    history_page(app, query, lane.unwrap_or_else(|| "search".into()), 0, limit).await.map(|p| p.hits)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDetail {
    summary: Summary,
    runs: Vec<Run>,
}

/// 一条命令的详情：汇总和最近 20 次执行
#[tauri::command]
async fn history_detail(app: tauri::AppHandle, text: String) -> CmdResult<HistoryDetail> {
    on_history(&app, move |s| Ok(HistoryDetail { summary: s.summary(&text)?, runs: s.runs(&text, 20)? })).await
}

#[tauri::command]
async fn history_hosts(app: tauri::AppHandle) -> CmdResult<Vec<HostCount>> {
    on_history(&app, |s| s.hosts()).await
}

#[tauri::command]
async fn history_favorite(app: tauri::AppHandle, text: String, on: bool) -> CmdResult<()> {
    let now = now_ms();
    on_history(&app, move |s| s.set_favorite(&text, on, now)).await
}

/// 删除一条命令的全部记录，或者（给了 id 时）只删那一次
#[tauri::command]
async fn history_delete(app: tauri::AppHandle, text: String, id: Option<i64>) -> CmdResult<usize> {
    on_history(&app, move |s| match id {
        Some(id) => s.delete_id(id),
        None => s.delete_text(&text),
    })
    .await
}

// ---- 设置 --------------------------------------------------------------------

#[tauri::command]
fn settings_get(state: State<'_, AppState>) -> serde_json::Value {
    state.settings.lock().unwrap().clone()
}

/// 写进 settings.json 的 JSON Schema 地址：编辑器打开文件时有补全和检查（schema/settings.schema.json）
const SETTINGS_SCHEMA: &str = "https://raw.githubusercontent.com/moonmxi/mooncli/main/schema/settings.schema.json";

fn with_schema(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut()
        && !obj.contains_key("$schema")
    {
        obj.insert("$schema".into(), serde_json::Value::String(SETTINGS_SCHEMA.into()));
    }
    value
}

#[tauri::command]
fn settings_set(state: State<'_, AppState>, value: serde_json::Value) -> CmdResult<()> {
    let value = with_schema(value);
    // 先按 Rust 端的结构检查一遍类型，写错了就拒绝，不把坏文件写到磁盘上
    let typed: Settings = serde_json::from_value(value.clone()).map_err(|e| format!("设置格式不对：{e}"))?;
    let shell_changed = typed.shell.path != state.typed_settings().shell.path;
    let text = serde_json::to_string_pretty(&value).map_err(err)?;
    // 先更新内存再写文件：监视设置文件的线程看到内容一致，就不会把自己的保存当成外部修改
    *state.settings.lock().unwrap() = value;
    files::write_atomic(&state.settings_path, text.as_bytes()).map_err(|e| format!("保存设置失败：{e}"))?;
    data::auto_snapshot(&state.data);
    for e in state.sessions.lock().unwrap().values() {
        e.sink.set_connect_commands(typed.remote.connect_commands.clone());
    }
    if shell_changed {
        *state.shell.lock().unwrap() = find_shell(typed.shell.path.as_deref());
    }
    agent::sync_notify_prefs(&state);
    Ok(())
}

/// 设置文件被别的程序改了（用户直接编辑 settings.json）：读进来通知界面；写坏了只报错，不覆盖用户的文件。
/// 自己保存时文件内容和内存里一致，不会被当成外部修改。
fn watch_settings_file(app: tauri::AppHandle, path: PathBuf) {
    use tauri::Emitter;
    std::thread::spawn(move || {
        let mtime = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut last = mtime(&path);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(800));
            let now = mtime(&path);
            if now == last {
                continue;
            }
            last = now;
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let state = app.state::<AppState>();
            let parsed = mooncli_config::jsonc::parse::<serde_json::Value>(&path.display().to_string(), &text)
                .map_err(|e| e.to_string())
                .and_then(|v| {
                    serde_json::from_value::<Settings>(v.clone()).map_err(|e| format!("设置格式不对：{e}"))?;
                    if v.is_object() { Ok(v) } else { Err("设置文件的顶层应该是一个对象".into()) }
                });
            match parsed {
                Ok(v) => {
                    if *state.settings.lock().unwrap() == v {
                        continue;
                    }
                    *state.settings.lock().unwrap() = v.clone();
                    let _ = app.emit("settings-changed", v);
                }
                Err(e) => {
                    let _ = app.emit("settings-error", e);
                }
            }
        }
    });
}

// ---- 数据管理（data.rs） ---------------------------------------------------------

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 换一份设置：先检查格式、备份当前的，再写入。返回写入后的设置，界面拿去替换
fn replace_settings(
    state: &State<'_, AppState>,
    value: serde_json::Value,
    reason: &str,
) -> CmdResult<serde_json::Value> {
    let _: Settings = serde_json::from_value(value.clone()).map_err(|e| format!("设置格式不对：{e}"))?;
    let value = with_schema(value);
    data::snapshot_settings(&state.data, reason).map_err(|e| format!("备份当前设置失败：{e}"))?;
    let text = serde_json::to_string_pretty(&value).map_err(err)?;
    *state.settings.lock().unwrap() = value.clone();
    files::write_atomic(&state.settings_path, text.as_bytes()).map_err(|e| format!("保存设置失败：{e}"))?;
    Ok(value)
}

#[tauri::command]
fn data_overview(state: State<'_, AppState>) -> data::Overview {
    let count = state.history_service().and_then(|s| s.count().ok());
    data::overview(&state.data, count)
}

#[tauri::command]
fn data_backup_now(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    data::snapshot_settings(&state.data, "manual").map_err(err)
}

#[tauri::command]
fn data_restore_backup(state: State<'_, AppState>, name: String) -> CmdResult<serde_json::Value> {
    let v = data::read_backup(&state.data, &name)?;
    replace_settings(&state, v, "before-restore")
}

/// 恢复默认设置（先备份当前的）
#[tauri::command]
fn settings_reset(state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    replace_settings(&state, serde_json::json!({}), "before-reset")
}

/// 导出备份包，返回文件路径
#[tauri::command]
fn data_export(state: State<'_, AppState>, include_history: bool, label: String) -> CmdResult<String> {
    let tmp = state.data.data_dir.join("export-history.tmp.db");
    let history = if include_history {
        let svc = state.history_service().ok_or("历史库还在加载，请稍等")?;
        svc.read(|s| s.backup_to(&tmp)).map_err(err)?;
        Some(tmp.as_path())
    } else {
        None
    };
    let r = data::export(&state.data, history, &label);
    let _ = std::fs::remove_file(&tmp);
    r.map(|p| p.display().to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportSummary {
    settings: Option<serde_json::Value>,
    backgrounds: usize,
    history_added: Option<usize>,
}

/// 导入备份包：设置整份替换（先备份当前的），背景图加进来，命令历史合并（不重复）
#[tauri::command]
fn data_import(state: State<'_, AppState>, data_base64: String, include_history: bool) -> CmdResult<ImportSummary> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data_base64).map_err(|_| "文件数据损坏")?;
    let pkg = data::read_package(&bytes)?;
    // 背景图先存：重名但内容不同的会改名，设置里引用的名字跟着改
    let mut renamed = HashMap::new();
    for (name, b) in &pkg.backgrounds {
        if let Ok(saved) = files::save_background_bytes(&state.backgrounds_dir, name, b)
            && &saved != name
        {
            renamed.insert(name.clone(), saved);
        }
    }
    let settings = match pkg.settings {
        Some(mut v) => {
            if let Some(img) = v.pointer_mut("/background/image")
                && let Some(new) = img.as_str().and_then(|n| renamed.get(n))
            {
                *img = serde_json::Value::String(new.clone());
            }
            Some(replace_settings(&state, v, "before-import")?)
        }
        None => None,
    };
    let history_added = match (include_history, pkg.history) {
        (true, Some(h)) => {
            let tmp = state.data.data_dir.join("import-history.tmp.db");
            std::fs::write(&tmp, h).map_err(err)?;
            let r = state.history_service().map(|svc| svc.modify(|s| s.merge_from(&tmp)));
            let _ = std::fs::remove_file(&tmp);
            Some(r.ok_or("历史库还在加载，请稍等")?.map_err(|e| format!("合并命令历史失败：{e}"))?)
        }
        _ => None,
    };
    Ok(ImportSummary { settings, backgrounds: pkg.backgrounds.len(), history_added })
}

/// 在文件管理器里打开：config / data / export 三个文件夹，或刚导出的备份文件
#[tauri::command]
fn data_reveal(state: State<'_, AppState>, target: String) -> CmdResult<()> {
    let d = &state.data;
    let path = match target.as_str() {
        "config" => d.config_dir.clone(),
        "data" => d.data_dir.clone(),
        "export" => d.export_dir.clone(),
        other => {
            let p = PathBuf::from(other);
            let ok = p.parent() == Some(d.export_dir.as_path())
                && p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with("mooncli-backup-") || n.starts_with("mooncli-logs-")
                });
            if !ok {
                return Err("只能打开 mooncli 自己的文件夹".into());
            }
            p
        }
    };
    data::reveal(&path)
}

/// 删掉 days 天前的命令历史；days 为 0 时全部删掉
#[tauri::command]
async fn history_prune(app: tauri::AppHandle, days: u32) -> CmdResult<usize> {
    let cutoff = now_ms() - days as i64 * 86_400_000;
    on_history(&app, move |svc| svc.modify(|s| if days == 0 { s.clear_all() } else { s.prune_before(cutoff) })).await
}

// ---- 剪贴板（走系统接口，WebView 不会弹出权限提示） ------------------------------

fn with_clipboard<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
) -> CmdResult<T> {
    let mut guard = state.clipboard.lock().unwrap();
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板：{e}"))?);
    }
    f(guard.as_mut().unwrap()).map_err(|e| format!("剪贴板出错：{e}"))
}

#[tauri::command]
fn clipboard_read(state: State<'_, AppState>) -> CmdResult<String> {
    match with_clipboard(&state, |c| c.get_text()) {
        Ok(t) => Ok(t),
        // 剪贴板里是图片或者是空的
        Err(_) => Ok(String::new()),
    }
}

#[tauri::command]
fn clipboard_write(state: State<'_, AppState>, text: String) -> CmdResult<()> {
    with_clipboard(&state, |c| c.set_text(text))
}

// ---- 背景图 ------------------------------------------------------------------

#[tauri::command]
fn background_save(state: State<'_, AppState>, file_name: String, data_base64: String) -> CmdResult<String> {
    files::save_background(&state.backgrounds_dir, &file_name, &data_base64)
}

#[tauri::command]
fn background_load(state: State<'_, AppState>, name: String) -> CmdResult<String> {
    files::load_background(&state.backgrounds_dir, &name)
}

#[tauri::command]
fn background_list(state: State<'_, AppState>) -> Vec<String> {
    files::list_backgrounds(&state.backgrounds_dir)
}

#[tauri::command]
fn background_delete(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    files::delete_background(&state.backgrounds_dir, &name)
}

// ---- 诊断 --------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    app_version: &'static str,
    os: &'static str,
    os_version: String,
    shell_path: Option<String>,
    shell_kind: Option<String>,
    integration_enabled: bool,
    bundled_conpty: bool,
    settings_path: String,
    history_path: String,
    history_count: Option<i64>,
    backgrounds_dir: String,
}

#[tauri::command]
fn diagnostics(state: State<'_, AppState>) -> Diagnostics {
    let shell = state.shell.lock().unwrap().clone();
    Diagnostics {
        app_version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        os_version: state.os_version.clone(),
        shell_path: shell.as_ref().map(|s| s.program.display().to_string()),
        shell_kind: shell.as_ref().map(|s| format!("{:?}", s.kind)),
        integration_enabled: state.typed_settings().shell.integration,
        bundled_conpty: state.bundled_conpty,
        settings_path: state.settings_path.display().to_string(),
        history_path: state.history_path.display().to_string(),
        history_count: state.history_service().and_then(|s| s.count().ok()),
        backgrounds_dir: state.backgrounds_dir.display().to_string(),
    }
}

/// 界面里的错误（未捕获的异常等）记进本机日志
#[tauri::command]
fn log_error(state: State<'_, AppState>, message: String) {
    let message: String = message.chars().take(4000).collect();
    logs::append(&state.data.data_dir, "error", &message);
}

/// 把日志和诊断信息打成 zip 放到下载文件夹，返回路径（用户反馈问题时附上）
#[tauri::command]
fn logs_pack(state: State<'_, AppState>, label: String) -> CmdResult<String> {
    let diag = serde_json::to_value(diagnostics(state.clone())).map_err(err)?;
    logs::pack(&state.data.data_dir, &state.data.export_dir, &diag, &label).map(|p| p.display().to_string())
}

/// 系统版本号，比如 "10.0.19045.5011"。Windows 10 和较早的 Windows 11 自带的 ConPTY 会吞掉集成标记。
fn os_version() -> String {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(out) = std::process::Command::new("cmd").args(["/c", "ver"]).creation_flags(CREATE_NO_WINDOW).output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(v) = text.split_once("Version ").map(|(_, r)| r.trim().trim_end_matches(']').to_string()) {
                return v;
            }
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|s| s.trim().to_string()).unwrap_or_default()
    }
}

// ---- 启动 --------------------------------------------------------------------

fn psreadline_history_path() -> Option<PathBuf> {
    let p = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
            .join(r"Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt")
    };
    p.is_file().then_some(p)
}

/// 打开历史库：先在写连接上做一次性的清理和导入，再起搜索线程（它自己在后台分批建索引）。
fn open_history(path: &std::path::Path, import: bool, keep_days: u32) -> Option<HistoryService> {
    let prepare = |store: &Store| {
        // "只保留最近 N 天"：每次启动清理一次
        if keep_days > 0 {
            let _ = store.prune_before(now_ms() - keep_days as i64 * 86_400_000);
        }
        if import
            && let Some(p) = psreadline_history_path()
            && let Ok(content) = std::fs::read_to_string(&p)
        {
            let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok();
            match store.import_psreadline(&content, host.as_deref(), now_ms()) {
                Ok(n) if n > 0 => eprintln!("mooncli: 导入了 {n} 条 PSReadLine 历史"),
                Ok(_) => {}
                Err(e) => eprintln!("mooncli: 导入 PSReadLine 历史失败: {e}"),
            }
        }
    };
    HistoryService::open(path, Filter::default(), prepare).map_err(|e| eprintln!("mooncli: 打开历史库失败: {e}")).ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 记住主窗口的位置、大小、是否最大化（独立的 Agent 小窗不记）
        .plugin(tauri_plugin_window_state::Builder::default().with_denylist(&["agent"]).build())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            std::fs::create_dir_all(&data_dir)?;
            logs::install_panic_hook(data_dir.clone());
            let settings_path = config_dir.join("settings.json");
            let settings = files::read_settings(&settings_path);
            let typed: Settings = serde_json::from_value(settings.clone()).unwrap_or_default();
            // 必须在创建第一个会话之前
            let bundled_conpty = conpty::prefer_bundled(app.handle(), typed.shell.use_bundled_conpty);
            let history_path = data_dir.join("history.db");

            let history = Arc::new(Mutex::new(None));
            if typed.history.enabled {
                let h = history.clone();
                let p = history_path.clone();
                let import = typed.history.import_psreadline;
                let keep = typed.history.keep_days;
                // 历史库在后台打开，不拖慢启动
                std::thread::spawn(move || *h.lock().unwrap() = open_history(&p, import, keep));
            }
            let data_paths = data::DataPaths {
                config_dir: config_dir.clone(),
                data_dir: data_dir.clone(),
                settings: settings_path.clone(),
                backgrounds: config_dir.join("backgrounds"),
                backups: config_dir.join("backups"),
                history: history_path.clone(),
                export_dir: app
                    .path()
                    .download_dir()
                    .or_else(|_| app.path().home_dir())
                    .unwrap_or_else(|_| data_dir.clone()),
            };
            data::auto_snapshot(&data_paths);
            watch_settings_file(app.handle().clone(), settings_path.clone());
            app.manage(AppState {
                sessions: Mutex::new(HashMap::new()),
                shell: Mutex::new(find_shell(typed.shell.path.as_deref())),
                history,
                settings: Mutex::new(settings),
                settings_path,
                backgrounds_dir: config_dir.join("backgrounds"),
                history_path,
                clipboard: Mutex::new(None),
                bundled_conpty,
                os_version: os_version(),
                data: data_paths,
                agent: Arc::default(),
            });
            agent::start_alerts(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_create,
            agent::agent_usage,
            agent::agent_refresh_live,
            agent::agent_window_toggle,
            agent::agent_bridge_path,
            fonts::system_fonts,
            agent::agent_integration_status,
            agent::agent_integration_preview,
            agent::agent_integration_apply,
            agent::notify_toast,
            session_write,
            session_resize,
            session_ack,
            session_close,
            session_remote_command,
            shell_status,
            open_url,
            session_stats,
            history_search,
            history_page,
            history_detail,
            history_hosts,
            history_favorite,
            history_delete,
            settings_get,
            settings_set,
            clipboard_read,
            clipboard_write,
            background_save,
            background_load,
            background_list,
            background_delete,
            diagnostics,
            data_overview,
            data_backup_now,
            data_restore_backup,
            settings_reset,
            data_export,
            data_import,
            data_reveal,
            history_prune,
            log_error,
            logs_pack
        ])
        .run(tauri::generate_context!())
        .expect("启动 mooncli 失败");
}
