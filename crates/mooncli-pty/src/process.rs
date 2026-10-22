//! 进程树识别（docs/06 §4）：标签页里的 pwsh 现在跑着什么程序。
//!
//! 命令文字只告诉我们用户敲了什么（`npm run dev`、`./build.ps1`），真正在跑的可能是
//! 它拉起来的 node、cargo、ssh……这里从 pwsh 的进程往下找：每一层取最新创建的那个子进程，
//! 得到一条进程链（比如 `cmd.exe → node.exe → codex.exe`），界面按链上最有辨识度的程序定图标。

/// 进程链上的一个程序：可执行文件名（小写，不带路径）和进程号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub exe: String,
}

/// 不算"在跑的程序"的辅助进程：控制台宿主之类
const IGNORED: [&str; 3] = ["conhost.exe", "openconsole.exe", "csrss.exe"];

/// 从 root 往下的进程链（不含 root 自己）。没有子进程时为空。
pub fn child_chain(root: u32) -> Vec<ProcInfo> {
    let all = snapshot();
    chain_from(&all, root)
}

/// 纯逻辑部分：给定 (pid, 父 pid, 创建顺序, exe) 列表，沿最新的子进程往下走。
pub fn chain_from(all: &[(u32, u32, u64, String)], root: u32) -> Vec<ProcInfo> {
    let mut out = Vec::new();
    let mut cur = root;
    for _ in 0..8 {
        let next = all
            .iter()
            .filter(|(pid, ppid, _, exe)| *ppid == cur && *pid != cur && !IGNORED.contains(&exe.as_str()))
            .max_by_key(|(_, _, order, _)| *order);
        match next {
            Some((pid, _, _, exe)) => {
                out.push(ProcInfo { pid: *pid, exe: exe.clone() });
                cur = *pid;
            }
            None => break,
        }
    }
    out
}

#[cfg(windows)]
fn snapshot() -> Vec<(u32, u32, u64, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    // SAFETY: 标准的 Toolhelp 用法：句柄用完关闭，结构体按要求先填 dwSize
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        // Toolhelp 按创建顺序列出进程（新的在后面），用序号当"创建先后"
        let mut order = 0u64;
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
            out.push((entry.th32ProcessID, entry.th32ParentProcessID, order, exe));
            order += 1;
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
fn snapshot() -> Vec<(u32, u32, u64, String)> {
    // Linux / macOS 只在开发时跑测试用：读 /proc（没有就算了）
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return out };
    for e in dir.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else { continue };
        let (Some(l), Some(r)) = (stat.find('('), stat.rfind(')')) else { continue };
        let exe = stat[l + 1..r].to_lowercase();
        let rest: Vec<&str> = stat[r + 2..].split(' ').collect();
        let ppid = rest.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
        let start = rest.get(19).and_then(|p| p.parse().ok()).unwrap_or(0);
        out.push((pid, ppid, start, exe));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_newest_child_and_skips_console_hosts() {
        let all = vec![
            (10, 1, 0, "pwsh.exe".to_string()),
            (11, 10, 1, "conhost.exe".to_string()),
            (12, 10, 2, "cmd.exe".to_string()),
            (13, 12, 3, "node.exe".to_string()),
            (14, 13, 4, "codex.exe".to_string()),
            (15, 10, 5, "git.exe".to_string()),
        ];
        let chain: Vec<String> = chain_from(&all, 10).into_iter().map(|p| p.exe).collect();
        // 最新的直接子进程是 git.exe（比 cmd.exe 晚）
        assert_eq!(chain, vec!["git.exe"]);
        let chain: Vec<String> = chain_from(&all, 12).into_iter().map(|p| p.exe).collect();
        assert_eq!(chain, vec!["node.exe", "codex.exe"]);
        assert!(chain_from(&all, 99).is_empty());
    }

    #[test]
    fn real_snapshot_finds_own_children() {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sleep" })
            .args(if cfg!(windows) { vec!["/c", "ping -n 3 127.0.0.1 >nul"] } else { vec!["2"] })
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let chain = child_chain(std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        assert!(!chain.is_empty(), "应能看到自己拉起的子进程");
    }
}
