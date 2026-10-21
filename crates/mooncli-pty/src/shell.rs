//! 找到 pwsh 并拼出启动参数。规则见 docs/02-terminal-core.md 第 3 节。

use std::path::{Path, PathBuf};

use crate::SpawnOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellKind {
    Pwsh,
    /// Windows PowerShell 5.1，只在找不到 pwsh 7 时临时使用
    WindowsPowerShell,
    /// 开发时在 Linux/macOS 上跑界面用的普通 shell
    Other,
}

#[derive(Debug, Clone)]
pub struct ShellSpec {
    pub program: PathBuf,
    pub kind: ShellKind,
}

/// 按顺序找：设置里的路径 → PATH 里的 pwsh → 常见安装位置。
pub fn find_shell(configured: Option<&str>) -> Option<ShellSpec> {
    if let Some(p) = configured.map(PathBuf::from).filter(|p| p.is_file()) {
        let kind = kind_of(&p);
        return Some(ShellSpec { program: p, kind });
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    find_with(&path_dirs, env("ProgramFiles"), env("LOCALAPPDATA"), env("SHELL"), env("SystemRoot"))
}

fn find_with(
    path_dirs: &[PathBuf],
    program_files: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    unix_shell: Option<PathBuf>,
    system_root: Option<PathBuf>,
) -> Option<ShellSpec> {
    let exe = if cfg!(windows) { "pwsh.exe" } else { "pwsh" };
    let mut candidates: Vec<PathBuf> = path_dirs.iter().map(|d| d.join(exe)).collect();
    if let Some(pf) = program_files {
        candidates.push(pf.join("PowerShell").join("7").join("pwsh.exe"));
        candidates.push(pf.join("PowerShell").join("7-preview").join("pwsh.exe"));
    }
    if let Some(la) = local_app_data {
        // Microsoft Store 版
        candidates.push(la.join("Microsoft").join("WindowsApps").join("pwsh.exe"));
    }
    if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(ShellSpec { program: p, kind: ShellKind::Pwsh });
    }
    if cfg!(windows) {
        let root = system_root.unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let p = root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        return p.is_file().then_some(ShellSpec { program: p, kind: ShellKind::WindowsPowerShell });
    }
    let sh = unix_shell.filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("/bin/sh"));
    Some(ShellSpec { kind: kind_of(&sh), program: sh })
}

fn kind_of(p: &Path) -> ShellKind {
    let name = p.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match name.as_str() {
        "pwsh" => ShellKind::Pwsh,
        "powershell" => ShellKind::WindowsPowerShell,
        _ => ShellKind::Other,
    }
}

/// 16 字节随机数的十六进制，作为 shell 集成的 nonce。
pub fn new_nonce() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("系统随机数不可用");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub struct LaunchRequest<'a> {
    pub shell: &'a ShellSpec,
    pub extra_args: &'a [String],
    /// 集成脚本 MoonCli.ps1 的路径；`None` 表示不开集成
    pub integration_script: Option<&'a Path>,
    pub session_id: &'a str,
    pub cwd: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub app_version: &'a str,
}

/// 拼出完整的启动参数和环境变量。
pub fn launch(req: LaunchRequest) -> SpawnOptions {
    let is_ps = matches!(req.shell.kind, ShellKind::Pwsh | ShellKind::WindowsPowerShell);
    let script = req.integration_script.filter(|_| is_ps);
    let nonce = script.map(|_| new_nonce());
    let mut args: Vec<String> = Vec::new();
    if is_ps {
        args.push("-NoLogo".into());
    }
    args.extend(req.extra_args.iter().cloned());
    if let Some(s) = script {
        // 用户 profile 照常先加载；集成脚本出错不影响使用（docs/02 §3）
        let quoted = s.display().to_string().replace('\'', "''");
        args.extend(["-NoExit".into(), "-Command".into(), format!("try {{ . '{quoted}' }} catch {{ }}")]);
    }
    let mut env = vec![
        ("TERM_PROGRAM".to_string(), "mooncli".to_string()),
        ("TERM_PROGRAM_VERSION".to_string(), req.app_version.to_string()),
        ("COLORTERM".to_string(), "truecolor".to_string()),
        ("MOONCLI_SESSION_ID".to_string(), req.session_id.to_string()),
    ];
    if !cfg!(windows) {
        env.push(("TERM".into(), "xterm-256color".into()));
    }
    if let Some(n) = &nonce {
        env.push(("MOONCLI_NONCE".into(), n.clone()));
    }
    SpawnOptions {
        program: req.shell.program.display().to_string(),
        args,
        cwd: req.cwd,
        env,
        cols: req.cols,
        rows: req.rows,
        nonce,
        flow_control: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, rel: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn prefers_path_then_program_files() {
        let d = std::env::temp_dir().join(format!("mooncli-shell-test-{}", new_nonce()));
        let exe = if cfg!(windows) { "pwsh.exe" } else { "pwsh" };
        let in_path = touch(&d, &format!("bin/{exe}"));
        let in_pf = touch(&d, "pf/PowerShell/7/pwsh.exe");
        let found = find_with(&[d.join("bin")], Some(d.join("pf")), None, None, None).unwrap();
        assert_eq!((found.program, found.kind), (in_path.clone(), ShellKind::Pwsh));
        std::fs::remove_file(&in_path).unwrap();
        let found = find_with(&[d.join("bin")], Some(d.join("pf")), None, None, None).unwrap();
        assert_eq!(found.program, in_pf);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn nonce_is_random_hex() {
        let (a, b) = (new_nonce(), new_nonce());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn pwsh_launch_has_integration_and_env() {
        let shell = ShellSpec { program: PathBuf::from(r"C:\pf\pwsh.exe"), kind: ShellKind::Pwsh };
        let script = PathBuf::from(r"C:\Program Files\moon's\MoonCli.ps1");
        let o = launch(LaunchRequest {
            shell: &shell,
            extra_args: &[],
            integration_script: Some(&script),
            session_id: "tab-1",
            cwd: None,
            cols: 120,
            rows: 32,
            app_version: "0.1.0",
        });
        assert_eq!(o.args[0], "-NoLogo");
        assert_eq!(o.args[1..3], ["-NoExit", "-Command"]);
        assert!(o.args[3].contains("moon''s"), "单引号要转义: {}", o.args[3]);
        let nonce = o.nonce.clone().unwrap();
        assert!(o.env.contains(&("MOONCLI_NONCE".into(), nonce)));
        assert!(o.env.contains(&("MOONCLI_SESSION_ID".into(), "tab-1".into())));
    }

    #[test]
    fn other_shell_gets_no_integration() {
        let shell = ShellSpec { program: PathBuf::from("/bin/bash"), kind: ShellKind::Other };
        let o = launch(LaunchRequest {
            shell: &shell,
            extra_args: &[],
            integration_script: Some(Path::new("/x/MoonCli.ps1")),
            session_id: "t",
            cwd: None,
            cols: 80,
            rows: 24,
            app_version: "0",
        });
        assert!(o.nonce.is_none());
        assert!(o.args.is_empty());
    }
}
