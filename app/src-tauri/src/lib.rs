//! mooncli 主程序：窗口、会话管理、历史库。界面在 app/src/。

mod conpty;
mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mooncli_history::{Hit, Query, Store};
use mooncli_pty::Session;
use mooncli_pty::shell::{LaunchRequest, ShellSpec, find_shell, launch};
use session::{UiMsg, UiSink};
use tauri::ipc::Channel;
use tauri::{Manager, State};

struct AppState {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    shell: Option<ShellSpec>,
    script: Option<PathBuf>,
    history: Arc<Mutex<Option<Store>>>,
}

type CmdResult<T> = Result<T, String>;

#[tauri::command]
fn session_create(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
    channel: Channel<UiMsg>,
) -> CmdResult<()> {
    let shell = state
        .shell
        .as_ref()
        .ok_or("没有找到 PowerShell 7（pwsh.exe）。请先安装：winget install Microsoft.PowerShell")?;
    let cwd = cwd.or_else(|| std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok());
    let opts = launch(LaunchRequest {
        shell,
        extra_args: &[],
        integration_script: state.script.as_deref(),
        session_id: &id,
        cwd,
        cols,
        rows,
        app_version: env!("CARGO_PKG_VERSION"),
    });
    let sink = Arc::new(UiSink::new(id.clone(), channel, state.history.clone()));
    let session = Session::spawn(opts, sink).map_err(|e| e.to_string())?;
    // 同一个 id 再创建（比如界面重新加载）时，旧会话随 drop 结束
    state.sessions.lock().unwrap().insert(id, Arc::new(session));
    Ok(())
}

fn with_session<T>(state: &State<'_, AppState>, id: &str, f: impl FnOnce(&Session) -> T) -> CmdResult<T> {
    let s = state.sessions.lock().unwrap().get(id).cloned().ok_or_else(|| format!("会话 {id} 不存在"))?;
    Ok(f(&s))
}

#[tauri::command]
fn session_write(state: State<'_, AppState>, id: String, data: String) -> CmdResult<()> {
    with_session(&state, &id, |s| s.write_chunked(data.as_bytes()))?.map_err(|e| e.to_string())
}

#[tauri::command]
fn session_resize(state: State<'_, AppState>, id: String, cols: u16, rows: u16) -> CmdResult<()> {
    with_session(&state, &id, |s| s.resize(cols, rows))?.map_err(|e| e.to_string())
}

#[tauri::command]
fn session_ack(state: State<'_, AppState>, id: String, bytes: usize) -> CmdResult<()> {
    with_session(&state, &id, |s| s.ack(bytes))
}

#[tauri::command]
fn session_close(state: State<'_, AppState>, id: String) {
    state.sessions.lock().unwrap().remove(&id);
}

#[tauri::command]
fn history_search(state: State<'_, AppState>, query: Query) -> CmdResult<Vec<Hit>> {
    let h = state.history.lock().unwrap();
    let store = h.as_ref().ok_or("历史库没有打开")?;
    store.search(&query).map_err(|e| e.to_string())
}

/// 集成脚本：安装后在资源目录；开发时直接用仓库里的。
fn find_script(app: &tauri::AppHandle) -> Option<PathBuf> {
    let installed = app.path().resolve("shell/MoonCli.ps1", tauri::path::BaseDirectory::Resource).ok();
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/pwsh/MoonCli.ps1");
    installed.into_iter().chain([dev]).find(|p| p.is_file())
}

fn psreadline_history_path() -> Option<PathBuf> {
    let p = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
            .join(r"Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt")
    };
    p.is_file().then_some(p)
}

fn open_history(app: &tauri::AppHandle) -> Option<Store> {
    let dir = app.path().app_local_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    let store = Store::open(dir.join("history.db")).map_err(|e| eprintln!("mooncli: 打开历史库失败: {e}")).ok()?;
    if let Some(p) = psreadline_history_path()
        && let Ok(content) = std::fs::read_to_string(&p)
    {
        let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        match store.import_psreadline(&content, host.as_deref(), now) {
            Ok(n) if n > 0 => eprintln!("mooncli: 导入了 {n} 条 PSReadLine 历史"),
            Ok(_) => {}
            Err(e) => eprintln!("mooncli: 导入 PSReadLine 历史失败: {e}"),
        }
    }
    // 提前建好内存索引，第一次打开历史面板就不用等
    let _ = store.search(&Query::new("", 0));
    Some(store)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            conpty::prefer_bundled(app.handle());
            let history = Arc::new(Mutex::new(None));
            let h = history.clone();
            let handle = app.handle().clone();
            // 历史库在后台打开，不拖慢启动
            std::thread::spawn(move || *h.lock().unwrap() = open_history(&handle));
            app.manage(AppState {
                sessions: Mutex::new(HashMap::new()),
                shell: find_shell(None),
                script: find_script(app.handle()),
                history,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_create,
            session_write,
            session_resize,
            session_ack,
            session_close,
            history_search
        ])
        .run(tauri::generate_context!())
        .expect("启动 mooncli 失败");
}
