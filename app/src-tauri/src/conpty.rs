//! 优先使用随安装包附带的新版 ConPTY（资源目录的 conpty/ 子目录，打包前由 tools/fetch_conpty.py 下载）。

/// 返回是否用上了附带的版本。设置里关掉"使用附带的 ConPTY"时不加载，退回系统自带的。
pub fn prefer_bundled(app: &tauri::AppHandle, enabled: bool) -> bool {
    use tauri::Manager;
    if !enabled {
        return false;
    }
    let Ok(dir) = app.path().resolve("conpty", tauri::path::BaseDirectory::Resource) else { return false };
    let ok = mooncli_pty::conpty::use_conpty_from(&dir);
    if cfg!(windows) && !ok {
        eprintln!("mooncli: 没有找到附带的 ConPTY（{}），使用系统自带的", dir.display());
    }
    ok
}
