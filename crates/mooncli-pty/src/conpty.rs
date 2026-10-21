//! 优先使用附带的新版 ConPTY（docs/02 §1）。
//!
//! portable-pty 第一次创建伪终端时，按系统的 DLL 搜索顺序加载 `conpty.dll`，找不到就用系统自带的。
//! 把附带的 conpty.dll 和 OpenConsole.exe 所在目录用 SetDllDirectoryW 加进搜索路径，就会用上附带的版本。
//! 必须在创建第一个会话之前调用。

use std::path::Path;

/// 返回是否用上了 `dir` 里的 ConPTY。
#[cfg(windows)]
pub fn use_conpty_from(dir: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    if !dir.join("conpty.dll").is_file() || !dir.join("OpenConsole.exe").is_file() {
        return false;
    }
    // 去掉 `\\?\` 前缀：Tauri 在 Windows 上给出的资源路径常带这个前缀
    let s = dir.as_os_str().to_string_lossy();
    let plain = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let wide: Vec<u16> = std::ffi::OsStr::new(plain).encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: 传入以 0 结尾的 UTF-16 路径，函数只读取它
    unsafe { windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW(wide.as_ptr()) != 0 }
}

#[cfg(not(windows))]
pub fn use_conpty_from(_dir: &Path) -> bool {
    false
}
