//! 识别"连接到别的机器"的命令：ssh、mosh、WSL、docker exec -it 等。规则见 docs/06-smart-tabs.md 第 4.1 节。
//!
//! 这类命令本身会一直运行到断开，如果当作普通命令计时，侧栏会永远显示"运行中"。
//! 所以识别出来以后换一套记法：连上就算这条命令完成，之后记录在远程执行的命令。

/// 一条连接命令要连到哪里。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectTarget {
    /// 用什么连的：ssh、mosh、wsl、docker……
    pub via: String,
    /// 显示用的主机名（容器名、WSL 发行版名）
    pub host: String,
    pub user: Option<String>,
}

/// 是交互式连接就返回目标；带了远程命令（`ssh host ls`）、只做端口转发（`ssh -N`）这类不算。
pub fn parse_connect(command: &str) -> Option<ConnectTarget> {
    parse_connect_with(command, &[])
}

/// 同上，`extra` 里的命令名也按 ssh 的写法识别（用户在设置里加的别名、自己写的脚本）。
pub fn parse_connect_with(command: &str, extra: &[String]) -> Option<ConnectTarget> {
    let words = split_words(command.trim().trim_start_matches('&').trim_start());
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    parse_words(&words, extra)
}

fn parse_words(words: &[&str], extra: &[String]) -> Option<ConnectTarget> {
    let (prog, args) = words.split_first()?;
    let prog = program_name(prog);
    let args = args.to_vec();
    if extra.iter().any(|e| e.eq_ignore_ascii_case(&prog)) {
        return parse_ssh(&args).map(|t| ConnectTarget { via: prog.clone(), ..t });
    }
    match prog.as_str() {
        "ssh" => parse_ssh(&args),
        // sshp：常见的 ssh 包装（带密码管理、跳板机的脚本、别名），参数和 ssh 一样
        "sshp" => parse_ssh(&args).map(|t| ConnectTarget { via: "sshp".into(), ..t }),
        // sshpass -p 密码 ssh host：去掉 sshpass 自己的参数，看后面的命令
        "sshpass" => {
            let mut i = 0;
            while i < args.len() && args[i].starts_with('-') {
                // -p 密码、-f 文件、-d 描述符、-P 提示词 后面跟一个参数；也可能直接连着写（-pSECRET）
                let takes_value = matches!(args[i], "-p" | "-f" | "-d" | "-P");
                i += if takes_value { 2 } else { 1 };
            }
            parse_words(args.get(i..)?, extra)
        }
        "mosh" | "et" => {
            let rest = positionals(&args, "");
            let dest = rest.first()?;
            Some(target(&prog, dest, None))
        }
        "plink" | "putty" => {
            // plink 带 -m（命令文件）或第二个位置参数时是执行命令
            if args.iter().any(|a| matches!(*a, "-m" | "-batch" | "-N" | "-nc")) {
                return None;
            }
            let rest = positionals(&args, "PlpwiDLRsoP");
            match rest.as_slice() {
                [dest] => Some(target(&prog, dest, flag_value(&args, "-l"))),
                _ => None,
            }
        }
        "telnet" => positionals(&args, "").first().map(|h| target("telnet", h, None)),
        "wsl" => parse_wsl(&args),
        "docker" | "podman" | "nerdctl" => parse_container_exec(&prog, &args),
        "kubectl" | "oc" => parse_kubectl(&prog, &args),
        "gcloud" => {
            // gcloud compute ssh NAME [--zone …]，带 --command 的是执行命令
            if args.len() < 3 || args[0] != "compute" || args[1] != "ssh" || has_long(&args, "--command") {
                return None;
            }
            let rest = positionals(&args[2..], "");
            rest.first().map(|d| target("gcloud", d, None))
        }
        "az" => {
            if args.len() < 2 || args[0] != "ssh" || !matches!(args[1], "vm" | "arc") {
                return None;
            }
            let host = long_value(&args, &["--name", "-n", "--ip", "--vm-name"]).unwrap_or("Azure");
            Some(target("az", host, long_value(&args, &["--local-user"])))
        }
        "aws" => {
            if args.len() < 2 || args[0] != "ssm" || args[1] != "start-session" {
                return None;
            }
            Some(target("aws", long_value(&args, &["--target"]).unwrap_or("AWS"), None))
        }
        "vagrant" => {
            if args.first() != Some(&"ssh") || has_long(&args, "-c") || has_long(&args, "--command") {
                return None;
            }
            let rest = positionals(&args[1..], "");
            Some(target("vagrant", rest.first().copied().unwrap_or("vagrant"), None))
        }
        "multipass" => {
            if args.first() != Some(&"shell") {
                return None;
            }
            let rest = positionals(&args[1..], "");
            Some(target("multipass", rest.first().copied().unwrap_or("primary"), None))
        }
        _ => None,
    }
}

fn target(via: &str, dest: &str, user: Option<&str>) -> ConnectTarget {
    let (user_in_dest, host) = split_dest(dest);
    ConnectTarget { via: via.into(), host, user: user.or(user_in_dest).map(str::to_string) }
}

/// `user@host`、`ssh://user@host:2222`、`host:path` → (user, host)
fn split_dest(dest: &str) -> (Option<&str>, String) {
    let d = dest.strip_prefix("ssh://").unwrap_or(dest);
    let (user, rest) = match d.rsplit_once('@') {
        Some((u, h)) => (Some(u).filter(|u| !u.is_empty()), h),
        None => (None, d),
    };
    // 去掉端口；IPv6 写成 [::1]:22
    let host = if let Some(v6) = rest.strip_prefix('[') {
        v6.split(']').next().unwrap_or(v6)
    } else if rest.matches(':').count() == 1 {
        rest.split(':').next().unwrap_or(rest)
    } else {
        rest
    };
    (user, host.trim_end_matches('/').to_string())
}

/// OpenSSH 的选项：这些字母后面要跟一个参数。
const SSH_ARG_FLAGS: &str = "BbcDEeFIiJLlmOoPpQRSWw";

fn parse_ssh(args: &[&str]) -> Option<ConnectTarget> {
    let mut user = None;
    let mut tty = false;
    let mut dest = None;
    let mut extra = 0;
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        i += 1;
        if dest.is_some() {
            extra += 1;
            continue;
        }
        if a == "--" {
            continue;
        }
        let Some(flags) = a.strip_prefix('-').filter(|f| !f.is_empty()) else {
            dest = Some(a);
            continue;
        };
        // 像 getopt 一样拆：-tt、-p22、-vp 22
        for (pos, c) in flags.char_indices() {
            match c {
                // 只转发端口、后台运行、打印配置、查询、控制已有连接：都不是交互式登录
                'N' | 'f' | 'G' | 'V' | 'Q' | 'O' | 'W' | 's' => return None,
                't' => tty = true,
                _ => {}
            }
            if SSH_ARG_FLAGS.contains(c) {
                let attached = &flags[pos + c.len_utf8()..];
                let value = if attached.is_empty() {
                    i += 1;
                    args.get(i - 1).copied()
                } else {
                    Some(attached)
                };
                if c == 'l' {
                    user = value;
                }
                break;
            }
        }
    }
    let dest = dest?;
    // ssh host ls 是在远程执行一条命令；加了 -t 的（ssh -t host tmux attach）还是交互式的
    if extra > 0 && !tty {
        return None;
    }
    Some(target("ssh", dest, user))
}

fn parse_wsl(args: &[&str]) -> Option<ConnectTarget> {
    let mut distro = None;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-d" | "--distribution" => {
                distro = args.get(i + 1).copied();
                i += 2;
            }
            "-u" | "--user" | "--cd" => i += 2,
            "~" => i += 1,
            // 执行命令（wsl ls、wsl -e …）、管理发行版（--shutdown、--list）都不算进入 shell
            _ => return None,
        }
    }
    let user = args.iter().position(|a| matches!(*a, "-u" | "--user")).and_then(|p| args.get(p + 1).copied());
    Some(target("wsl", distro.unwrap_or("WSL"), user))
}

/// `docker exec -it NAME bash`：要有 -i 和 -t，后面跟的程序是 shell 才算
fn parse_container_exec(prog: &str, args: &[&str]) -> Option<ConnectTarget> {
    let sub = args.first()?;
    if *sub == "attach" {
        return positionals(&args[1..], "").first().map(|c| target(prog, c, None));
    }
    if *sub != "exec" && !(*sub == "container" && args.get(1) == Some(&"exec")) {
        return None;
    }
    let rest = if *sub == "exec" { &args[1..] } else { &args[2..] };
    let (i, t) = interactive_flags(rest);
    if !(i && t) {
        return None;
    }
    let pos = positionals(rest, "euwU");
    let (name, cmd) = pos.split_first()?;
    let shell = cmd.first().map(|c| program_name(c))?;
    is_shell(&shell).then(|| target(prog, name, flag_value(rest, "-u").or(long_value(rest, &["--user"]))))
}

/// `kubectl exec -it POD -- sh`
fn parse_kubectl(prog: &str, args: &[&str]) -> Option<ConnectTarget> {
    if args.first() != Some(&"exec") {
        return None;
    }
    let rest = &args[1..];
    let (i, t) = interactive_flags(rest);
    let dash = rest.iter().position(|a| *a == "--")?;
    let shell = program_name(rest.get(dash + 1)?);
    if !(i && t && is_shell(&shell)) {
        return None;
    }
    let pos = positionals(&rest[..dash], "ncC");
    pos.first().map(|p| target(prog, p.trim_start_matches("pod/"), None))
}

fn interactive_flags(args: &[&str]) -> (bool, bool) {
    let (mut i, mut t) = (false, false);
    for a in args {
        match *a {
            "--interactive" => i = true,
            "--tty" => t = true,
            s if s.starts_with('-') && !s.starts_with("--") => {
                i |= s.contains('i');
                t |= s.contains('t');
            }
            _ => {}
        }
    }
    (i, t)
}

fn is_shell(p: &str) -> bool {
    matches!(p, "sh" | "bash" | "zsh" | "fish" | "ash" | "dash" | "ksh" | "pwsh" | "powershell" | "cmd" | "nu")
}

/// 去掉选项后剩下的位置参数。`with_arg` 是后面要跟参数的短选项字母；长选项写成 `--x=y` 或 `--x y` 都能跳过。
fn positionals<'a>(args: &[&'a str], with_arg: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        i += 1;
        if a == "--" {
            out.extend_from_slice(&args[i..]);
            break;
        }
        if let Some(long) = a.strip_prefix("--") {
            // 不知道长选项要不要参数：下一个词不是选项、而且后面还有词时，当作它的参数
            if !long.contains('=') && LONG_WITH_ARG.contains(&long) {
                i += 1;
            }
            continue;
        }
        if let Some(f) = a.strip_prefix('-').filter(|f| !f.is_empty()) {
            if f.len() == 1 && with_arg.contains(f) {
                i += 1;
            }
            continue;
        }
        out.push(a);
    }
    out
}

/// 上面各命令里常见的、要跟参数的长选项
const LONG_WITH_ARG: &[&str] = &[
    "zone",
    "project",
    "user",
    "workdir",
    "env",
    "namespace",
    "container",
    "context",
    "ssh",
    "port",
    "server",
    "predict",
    "local-user",
    "resource-group",
    "target",
    "name",
    "ip",
    "vm-name",
    "distribution",
    "cd",
    "family",
];

fn flag_value<'a>(args: &[&'a str], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| *a == flag).and_then(|p| args.get(p + 1).copied())
}

fn long_value<'a>(args: &[&'a str], names: &[&str]) -> Option<&'a str> {
    for (i, a) in args.iter().enumerate() {
        for n in names {
            if a == n {
                return args.get(i + 1).copied();
            }
            if let Some(v) = a.strip_prefix(n).and_then(|r| r.strip_prefix('=')) {
                return Some(v);
            }
        }
    }
    None
}

fn has_long(args: &[&str], name: &str) -> bool {
    args.iter().any(|a| *a == name || a.strip_prefix(name).is_some_and(|r| r.starts_with('=')))
}

/// `C:\Windows\System32\OpenSSH\ssh.exe` → ssh
fn program_name(word: &str) -> String {
    let base = word.rsplit(['\\', '/']).next().unwrap_or(word);
    let lower = base.to_ascii_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}

/// 按空白拆词，支持单双引号（够识别命令用，不追求和 pwsh 完全一致）。
fn split_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut has = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            // 遇到管道、分号就停：后面是别的命令
            (None, '|' | ';') => break,
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 远程输出的最后一行（还没换行的那部分）像不像 shell 提示符。
/// 用于远程 shell 既不发集成标记、也不开括号粘贴（比如 CentOS 7 的 bash 4.2）时判断"已经连上"。
pub fn looks_like_prompt(line: &str) -> bool {
    let t = line.trim_end();
    if t.is_empty() || t.chars().count() > 160 {
        return false;
    }
    let lower = t.to_lowercase();
    if ["password", "passphrase", "密码", "verification", "yes/no", "otp", "token", "pin:"]
        .iter()
        .any(|w| lower.contains(w))
    {
        return false;
    }
    let last = t.chars().last().unwrap_or(' ');
    matches!(last, '$' | '#' | '%' | '>' | '❯' | '➜' | 'λ' | '»' | '›')
}

/// 很多发行版的 bash 默认把窗口标题设成 `user@host: ~/dir`（Ubuntu、Debian）或 `user@host:~/dir`。
/// 远程没装集成、不发 OSC 7 时，从这里拿远程的当前目录。
pub fn cwd_from_title(title: &str) -> Option<(String, String)> {
    let (who, path) = title.trim().split_once(':')?;
    let (_, host) = who.split_once('@')?;
    let path = path.trim();
    let ok =
        !host.is_empty() && !host.contains(char::is_whitespace) && (path.starts_with('/') || path.starts_with('~'));
    ok.then(|| (host.to_string(), path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(cmd: &str) -> Option<(String, String, Option<String>)> {
        parse_connect(cmd).map(|t| (t.via, t.host, t.user))
    }

    #[test]
    fn ssh_variants() {
        let s = |h: &str, u: Option<&str>| Some(("ssh".to_string(), h.to_string(), u.map(str::to_string)));
        assert_eq!(host("ssh dev@10.0.0.5"), s("10.0.0.5", Some("dev")));
        assert_eq!(host("ssh -p 2222 -i ~/.ssh/key box"), s("box", None));
        assert_eq!(host("ssh -p2222 -l root box"), s("box", Some("root")));
        assert_eq!(host("ssh -vp 22 box"), s("box", None));
        assert_eq!(host("ssh -J jump@bastion -o ServerAliveInterval=30 me@inner"), s("inner", Some("me")));
        assert_eq!(host("ssh ssh://me@host.example:2200"), s("host.example", Some("me")));
        assert_eq!(host(r#"& "C:\Windows\System32\OpenSSH\ssh.exe" box"#), s("box", None));
        assert_eq!(host("ssh -t box tmux attach"), s("box", None));
        assert_eq!(host("SSH.EXE box"), s("box", None));
    }

    #[test]
    fn ssh_that_is_not_a_login() {
        for c in [
            "ssh box ls -la",
            "ssh -N -L 8080:localhost:80 box",
            "ssh -f box sleep 9",
            "ssh -G box",
            "ssh -V",
            "ssh",
            "ssh -O exit box",
        ] {
            assert_eq!(parse_connect(c), None, "{c}");
        }
    }

    #[test]
    fn ssh_wrappers_and_aliases() {
        assert_eq!(host("sshp dev@10.0.0.5").unwrap(), ("sshp".into(), "10.0.0.5".into(), Some("dev".into())));
        assert_eq!(host("sshp -p 2222 box").unwrap().1, "box");
        assert_eq!(host("sshp box uptime"), None);
        assert_eq!(host("sshpass -p secret ssh me@box").unwrap(), ("ssh".into(), "box".into(), Some("me".into())));
        assert_eq!(host("sshpass -e ssh -p 22 box").unwrap().1, "box");
        assert_eq!(host("sshpass -psecret ssh box ls"), None);
        let extra = vec!["jump".to_string()];
        assert_eq!(
            parse_connect_with("jump -l root prod-1", &extra).unwrap(),
            ConnectTarget { via: "jump".into(), host: "prod-1".into(), user: Some("root".into()) }
        );
        assert_eq!(parse_connect("jump prod-1"), None);
    }

    #[test]
    fn other_tools() {
        assert_eq!(host("mosh me@box").unwrap().1, "box");
        assert_eq!(host("wsl").unwrap().1, "WSL");
        assert_eq!(host("wsl -d Ubuntu-22.04").unwrap().1, "Ubuntu-22.04");
        assert_eq!(host("wsl ls"), None);
        assert_eq!(host("wsl --shutdown"), None);
        assert_eq!(host("docker exec -it web bash").unwrap().1, "web");
        assert_eq!(
            host("docker exec -it -u root web /bin/sh").unwrap(),
            ("docker".into(), "web".into(), Some("root".into()))
        );
        assert_eq!(host("docker exec web ls"), None);
        assert_eq!(host("docker exec -it web python manage.py shell"), None);
        assert_eq!(host("kubectl exec -it -n prod pod/api-7d9 -- bash").unwrap().1, "api-7d9");
        assert_eq!(host("kubectl exec api -- ls"), None);
        assert_eq!(host("gcloud compute ssh vm-1 --zone us-east1-b").unwrap().1, "vm-1");
        assert_eq!(host("gcloud compute ssh vm-1 --command=uptime"), None);
        assert_eq!(host("vagrant ssh").unwrap().1, "vagrant");
        assert_eq!(host("multipass shell dev").unwrap().1, "dev");
        assert_eq!(host("aws ssm start-session --target i-0abc").unwrap().1, "i-0abc");
        assert_eq!(host("plink -l me box").unwrap(), ("plink".into(), "box".into(), Some("me".into())));
        assert_eq!(host("git status"), None);
        assert_eq!(host("ssh box | tee log").unwrap().1, "box");
    }

    #[test]
    fn cwd_in_title() {
        assert_eq!(cwd_from_title("dev@vm: /tmp"), Some(("vm".into(), "/tmp".into())));
        assert_eq!(cwd_from_title("root@web-1:~/app"), Some(("web-1".into(), "~/app".into())));
        assert_eq!(cwd_from_title("vim: notes.md"), None);
        assert_eq!(cwd_from_title("Claude Code"), None);
    }

    #[test]
    fn prompt_heuristic() {
        assert!(looks_like_prompt("[root@centos7 ~]# "));
        assert!(looks_like_prompt("me@box:~$ "));
        assert!(looks_like_prompt("box% "));
        assert!(looks_like_prompt("❯ "));
        assert!(!looks_like_prompt("me@box's password: "));
        assert!(!looks_like_prompt("Are you sure you want to continue connecting (yes/no/[fingerprint])? "));
        assert!(!looks_like_prompt("Enter passphrase for key '/home/me/.ssh/id_ed25519': "));
        assert!(!looks_like_prompt("Welcome to Ubuntu"));
    }
}
