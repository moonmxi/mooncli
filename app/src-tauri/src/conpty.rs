//! 优先使用随程序附带的新版 ConPTY（docs/02 §1）。
//!
//! portable-pty 第一次创建伪终端时会按系统的 DLL 搜索顺序加载 `conpty.dll`，找不到就用系统自带的。
//! 我们把附带的 conpty.dll 和 OpenConsole.exe 放在资源目录的 conpty/ 子目录里，
//! 启动时用 SetDllDirectoryW 把这个目录加进搜索路径；设置里关掉"使用附带的 ConPTY"时不加，就会退回系统版本。

#[cfg(windows)]
pub fn prefer_bundled(app: &tauri::AppHandle) {
    use std::os::windows::ffi::OsStrExt;
    use tauri::Manager;

    let Ok(dir) = app.path().resolve("conpty", tauri::path::BaseDirectory::Resource) else { return };
    if !dir.join("conpty.dll").is_file() {
        return;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: 传入以 0 结尾的 UTF-16 路径，函数只读取它
    let ok = unsafe { windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW(wide.as_ptr()) };
    if ok == 0 {
        eprintln!("mooncli: 设置 ConPTY 目录失败，使用系统自带的 ConPTY");
    }
}

#[cfg(not(windows))]
pub fn prefer_bundled(_app: &tauri::AppHandle) {}
