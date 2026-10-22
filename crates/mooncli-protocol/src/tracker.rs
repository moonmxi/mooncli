//! 会话状态机：决定"现在按键归谁"。规则见 docs/05-interaction.md 第 1 节。
//!
//! 只有带 nonce 的标记（本机 pwsh 集成脚本发的）能把会话切到"提示符上"。
//! ssh 到远程后远程 shell 发的标记不带 nonce，只当作远程信息，不会让 mooncli 开始拦按键。
//!
//! 连接类命令（ssh、mosh、wsl……）单独处理（docs/06 §4.1）：连上就算这条命令完成，
//! 之后用远程 shell 的标记或括号粘贴开关，分出在远程执行的一条条命令。

use crate::remote::{ConnectTarget, looks_like_prompt, parse_connect_with};
use crate::scanner::{MarkKind, ShellEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellState {
    /// 还没收到过集成标记（集成关闭、加载失败，或者还在启动）
    Unknown,
    /// 本机 pwsh 在等待输入
    Prompt,
    /// 本机 pwsh 正在执行命令（包括 ssh、claude 等长时间运行的程序）
    Running,
    /// 全屏程序（vim、less、远程 tmux）
    AltScreen,
}

/// 状态机对外报告的变化，界面据此更新标签页。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TrackerUpdate {
    State {
        state: ShellState,
    },
    /// 本机命令开始。`text` 来自私有序列 `cmd`，可能拿不到。
    CommandStarted {
        text: Option<String>,
    },
    CommandFinished {
        text: Option<String>,
        exit_code: Option<i32>,
    },
    /// 远程 shell（不带 nonce）的提示符，说明用户此刻在远程的命令行上。
    RemotePrompt,
    /// 刚开始的本机命令是一条连接命令，正在连接
    RemoteConnecting {
        target: ConnectTarget,
    },
    /// 连上了。连接命令到此算"完成"，之后的计时都给远程命令
    RemoteConnected {
        target: ConnectTarget,
    },
    /// 远程开始执行一条命令。命令文字由界面从屏幕上读出来（按 `seq` 对应）
    RemoteCommandStarted {
        seq: u64,
    },
    /// 远程命令结束。只有远程装了 shell 集成时才有退出码
    RemoteCommandFinished {
        seq: u64,
        exit_code: Option<i32>,
    },
    /// 连上之后断开了（用户 exit，或者网络断了）
    RemoteDisconnected {
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemotePhase {
    /// 连上了但还不知道在做什么（远程 shell 不发任何信号，或者正在全屏程序里）
    Unknown,
    Prompt,
    Running,
}

#[derive(Debug)]
struct Remote {
    target: ConnectTarget,
    connected: bool,
    phase: RemotePhase,
    /// 远程 shell 发过 133;C，以后只认标记，不再看括号粘贴
    marks: bool,
    tail: LineTail,
}

/// 记住输出里最后一行还没换行的文字（去掉控制序列），用来看它像不像提示符。
#[derive(Debug, Default)]
struct LineTail {
    text: Vec<u8>,
    esc: u8,
}

impl LineTail {
    const MAX: usize = 400;

    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match (self.esc, b) {
                // 0：普通文字；1：刚遇到 ESC；2：CSI；3：OSC 等字符串序列；4：字符串里遇到 ESC
                (0, 0x1b) => self.esc = 1,
                (0, b'\n' | b'\r') => self.text.clear(),
                (0, 0x08) => {
                    self.text.pop();
                }
                (0, b) if b >= 0x20 || b == b'\t' => {
                    if self.text.len() < Self::MAX {
                        self.text.push(b);
                    }
                }
                (0, _) => {}
                (1, b'[') => self.esc = 2,
                (1, b']' | b'P' | b'_' | b'^') => self.esc = 3,
                (1, _) => self.esc = 0,
                (2, 0x40..=0x7e) => self.esc = 0,
                (2, _) => {}
                (3, 0x07) => self.esc = 0,
                (3, 0x1b) => self.esc = 4,
                (3, _) => {}
                (4, b'\\') => self.esc = 0,
                (4, _) => self.esc = 3,
                _ => self.esc = 0,
            }
        }
    }

    fn looks_like_prompt(&self) -> bool {
        self.esc == 0 && looks_like_prompt(&String::from_utf8_lossy(&self.text))
    }
}

#[derive(Debug)]
pub struct Tracker {
    base: ShellState,
    alt: bool,
    pending_cmd: Option<String>,
    running_cmd: Option<String>,
    integrated: bool,
    remote: Option<Remote>,
    remote_seq: u64,
    /// 设置里加的、也算连接的命令名
    connect_commands: Vec<String>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            base: ShellState::Unknown,
            alt: false,
            pending_cmd: None,
            running_cmd: None,
            integrated: false,
            remote: None,
            remote_seq: 0,
            connect_commands: Vec::new(),
        }
    }
}

impl Tracker {
    pub fn state(&self) -> ShellState {
        if self.alt { ShellState::AltScreen } else { self.base }
    }

    /// 补全面板这类会拦截按键的功能，只在这里返回 true 时启用。
    pub fn terminal_may_handle_completion_keys(&self) -> bool {
        self.state() == ShellState::Prompt
    }

    /// 是否收到过本机集成脚本的标记。
    pub fn is_integrated(&self) -> bool {
        self.integrated
    }

    pub fn set_connect_commands(&mut self, names: Vec<String>) {
        self.connect_commands = names;
    }

    /// 正在连接或已经连上的远程主机
    pub fn remote(&self) -> Option<(&ConnectTarget, bool)> {
        self.remote.as_ref().map(|r| (&r.target, r.connected))
    }

    /// 普通输出也要看一眼：远程 shell 什么信号都不发时，靠"最后一行像提示符"判断已经连上。
    pub fn observe_output(&mut self, bytes: &[u8]) -> Vec<TrackerUpdate> {
        let mut out = Vec::new();
        if let Some(r) = self.remote.as_mut().filter(|r| !r.connected) {
            r.tail.feed(bytes);
            if !self.alt && r.tail.looks_like_prompt() {
                self.connect(&mut out);
            }
        }
        out
    }

    fn connect(&mut self, out: &mut Vec<TrackerUpdate>) {
        if let Some(r) = self.remote.as_mut().filter(|r| !r.connected) {
            r.connected = true;
            r.tail = LineTail::default();
            out.push(TrackerUpdate::RemoteConnected { target: r.target.clone() });
        }
    }

    /// 连上之后，远程 shell 的信号怎么变成"命令开始 / 结束"。
    fn remote_event(&mut self, ev: &ShellEvent, out: &mut Vec<TrackerUpdate>) {
        let alt = self.alt;
        let Some(r) = self.remote.as_mut() else { return };
        // 还没连上：远程 shell 的任何信号都说明已经登录进去了
        let ready = match ev {
            ShellEvent::BracketedPaste { on: true } => !alt,
            ShellEvent::Mark { trusted: false, .. } | ShellEvent::CommandEnd { trusted: false, .. } => true,
            ShellEvent::Cwd { host: Some(_), .. } => true,
            ShellEvent::AltScreen { on: true } => true,
            _ => false,
        };
        if !r.connected {
            if !ready {
                return;
            }
            self.connect(out);
        }
        let r = self.remote.as_mut().expect("上面刚确认过");
        if let ShellEvent::Mark { kind: MarkKind::CommandStart, trusted: false } = ev {
            r.marks = true;
        }
        let paste = |on: bool| matches!(ev, ShellEvent::BracketedPaste { on: o } if *o == on) && !r.marks;
        let (starts, ends, exit_code) = match ev {
            ShellEvent::Mark { kind: MarkKind::CommandStart, trusted: false } => (true, false, None),
            ShellEvent::Mark { kind: MarkKind::PromptStart | MarkKind::PromptEnd, trusted: false } => {
                (false, true, None)
            }
            ShellEvent::CommandEnd { exit_code, trusted: false } => (false, true, *exit_code),
            _ if paste(false) => (true, false, None),
            // 全屏程序（vim）自己也会开括号粘贴，不算回到提示符
            _ if paste(true) && !alt => (false, true, None),
            _ => (false, false, None),
        };
        match r.phase {
            RemotePhase::Running if ends => {
                r.phase = RemotePhase::Prompt;
                out.push(TrackerUpdate::RemoteCommandFinished { seq: self.remote_seq, exit_code });
            }
            RemotePhase::Prompt if starts => {
                r.phase = RemotePhase::Running;
                self.remote_seq += 1;
                out.push(TrackerUpdate::RemoteCommandStarted { seq: self.remote_seq });
            }
            RemotePhase::Unknown if ends || matches!(ev, ShellEvent::BracketedPaste { on: true }) && !alt => {
                r.phase = RemotePhase::Prompt;
            }
            _ => {}
        }
    }

    pub fn apply(&mut self, ev: &ShellEvent) -> Vec<TrackerUpdate> {
        let before = self.state();
        let mut out = Vec::new();
        if let ShellEvent::AltScreen { on } = ev {
            self.alt = *on;
        }
        if self.remote.is_some() {
            self.remote_event(ev, &mut out);
        }
        match ev {
            ShellEvent::Mark { kind, trusted: true } => {
                self.integrated = true;
                match kind {
                    MarkKind::PromptStart => {}
                    MarkKind::PromptEnd => self.base = ShellState::Prompt,
                    MarkKind::CommandStart => {
                        self.base = ShellState::Running;
                        self.running_cmd = self.pending_cmd.take();
                        out.push(TrackerUpdate::CommandStarted { text: self.running_cmd.clone() });
                        let extra = &self.connect_commands;
                        if let Some(target) = self.running_cmd.as_deref().and_then(|c| parse_connect_with(c, extra)) {
                            out.push(TrackerUpdate::RemoteConnecting { target: target.clone() });
                            self.remote = Some(Remote {
                                target,
                                connected: false,
                                phase: RemotePhase::Unknown,
                                marks: false,
                                tail: LineTail::default(),
                            });
                        }
                    }
                }
            }
            ShellEvent::CommandEnd { exit_code, trusted: true } => {
                self.integrated = true;
                match self.remote.take() {
                    // 连上过：连接命令早就算完成了，这里只是断开
                    Some(r) if r.connected => {
                        if r.phase == RemotePhase::Running {
                            out.push(TrackerUpdate::RemoteCommandFinished { seq: self.remote_seq, exit_code: None });
                        }
                        self.running_cmd = None;
                        out.push(TrackerUpdate::RemoteDisconnected { exit_code: *exit_code });
                    }
                    // 没连上（密码错、主机不通）就是一条失败（或被取消）的普通命令
                    _ => out
                        .push(TrackerUpdate::CommandFinished { text: self.running_cmd.take(), exit_code: *exit_code }),
                }
                // 命令结束时全屏程序肯定也退出了（防止程序崩溃没发 ?1049l）
                self.alt = false;
            }
            ShellEvent::Mark { kind: MarkKind::PromptEnd, trusted: false } => {
                out.push(TrackerUpdate::RemotePrompt);
            }
            ShellEvent::Private { kind, data } if kind == "cmd" => {
                self.pending_cmd = data.get("text").and_then(|t| t.as_str()).map(str::to_string);
            }
            _ => {}
        }
        let after = self.state();
        if after != before {
            out.insert(0, TrackerUpdate::State { state: after });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::Scanner;

    fn run(t: &mut Tracker, s: &mut Scanner, bytes: &[u8]) -> Vec<TrackerUpdate> {
        s.feed(bytes).events.iter().flat_map(|e| t.apply(e)).collect()
    }

    fn private(nonce: &str, kind: &str, json: &str) -> String {
        use base64::Engine;
        format!("\x1b]6973;{nonce};{kind};{}\x07", base64::engine::general_purpose::STANDARD.encode(json))
    }

    #[test]
    fn full_local_command_cycle() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        assert_eq!(t.state(), ShellState::Unknown);
        run(&mut t, &mut s, b"\x1b]133;A;nonce=n\x07PS> \x1b]133;B;nonce=n\x07");
        assert!(t.terminal_may_handle_completion_keys());

        let cmd = private("n", "cmd", r#"{"text":"git status"}"#);
        let ups = run(&mut t, &mut s, format!("{cmd}\x1b]133;C;nonce=n\x07").as_bytes());
        assert_eq!(
            ups,
            vec![
                TrackerUpdate::State { state: ShellState::Running },
                TrackerUpdate::CommandStarted { text: Some("git status".into()) }
            ]
        );
        assert!(!t.terminal_may_handle_completion_keys());

        let ups =
            run(&mut t, &mut s, b"output\r\n\x1b]133;D;0;nonce=n\x07\x1b]133;A;nonce=n\x07\x1b]133;B;nonce=n\x07");
        assert_eq!(
            ups,
            vec![
                TrackerUpdate::CommandFinished { text: Some("git status".into()), exit_code: Some(0) },
                TrackerUpdate::State { state: ShellState::Prompt },
            ]
        );
    }

    #[test]
    fn ssh_session_never_gives_keys_to_terminal() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        run(&mut t, &mut s, b"\x1b]133;B;nonce=n\x07\x1b]133;C;nonce=n\x07");
        // 远程 bash 也装了集成，发出不带 nonce 的提示符标记
        let ups = run(&mut t, &mut s, b"\x1b]133;A\x07user@box:~$ \x1b]133;B\x07");
        assert_eq!(ups, vec![TrackerUpdate::RemotePrompt]);
        assert_eq!(t.state(), ShellState::Running);
        assert!(!t.terminal_may_handle_completion_keys());
    }

    #[test]
    fn remote_program_cannot_forge_local_prompt() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("secret".into())));
        run(&mut t, &mut s, b"\x1b]133;C;nonce=secret\x07");
        run(&mut t, &mut s, b"\x1b]133;B;nonce=guess\x07");
        assert_eq!(t.state(), ShellState::Running);
    }

    #[test]
    fn alt_screen_overrides_and_is_cleared_on_command_end() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        run(&mut t, &mut s, b"\x1b]133;C;nonce=n\x07\x1b[?1049h");
        assert_eq!(t.state(), ShellState::AltScreen);
        // vim 被强制结束，没发 ?1049l
        run(&mut t, &mut s, b"\x1b]133;D;137;nonce=n\x07\x1b]133;B;nonce=n\x07");
        assert_eq!(t.state(), ShellState::Prompt);
    }

    #[test]
    fn without_integration_state_stays_unknown() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        run(&mut t, &mut s, b"PS C:\\> dir\r\n");
        assert_eq!(t.state(), ShellState::Unknown);
        assert!(!t.is_integrated());
        assert!(!t.terminal_may_handle_completion_keys());
    }

    fn start(t: &mut Tracker, s: &mut Scanner, cmd: &str) -> Vec<TrackerUpdate> {
        let p = private("n", "cmd", &format!(r#"{{"text":"{cmd}"}}"#));
        run(t, s, format!("\x1b]133;B;nonce=n\x07{p}\x1b]133;C;nonce=n\x07").as_bytes())
    }

    fn box_target() -> ConnectTarget {
        ConnectTarget { via: "ssh".into(), host: "box".into(), user: Some("me".into()) }
    }

    #[test]
    fn ssh_connects_then_tracks_remote_commands_by_bracketed_paste() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        let ups = start(&mut t, &mut s, "ssh me@box");
        assert_eq!(ups.last(), Some(&TrackerUpdate::RemoteConnecting { target: box_target() }));
        // 输密码不算连上
        assert!(run(&mut t, &mut s, b"me@box's password: ").is_empty());
        // bash 5 开始读输入：打开括号粘贴
        let ups = run(&mut t, &mut s, b"\r\nWelcome\r\n\x1b[?2004hme@box:~$ ");
        assert_eq!(ups, vec![TrackerUpdate::RemoteConnected { target: box_target() }]);
        // 回车：关掉括号粘贴，命令开始
        let ups = run(&mut t, &mut s, b"ls\r\n\x1b[?2004l\r");
        assert_eq!(ups, vec![TrackerUpdate::RemoteCommandStarted { seq: 1 }]);
        // 远程的 vim 也开括号粘贴，不算结束
        assert!(
            run(&mut t, &mut s, b"\x1b[?1049h\x1b[?2004h").iter().all(|u| matches!(u, TrackerUpdate::State { .. }))
        );
        run(&mut t, &mut s, b"\x1b[?2004l\x1b[?1049l");
        let ups = run(&mut t, &mut s, b"\x1b[?2004hme@box:~$ ");
        assert_eq!(ups, vec![TrackerUpdate::RemoteCommandFinished { seq: 1, exit_code: None }]);
        // 本机 pwsh 一直是"运行中"，按键不归 mooncli
        assert_eq!(t.state(), ShellState::Running);
        assert!(!t.terminal_may_handle_completion_keys());
        // exit：ssh 结束，只报断开，不再报一次命令完成
        let ups = run(&mut t, &mut s, b"exit\r\n\x1b[?2004l\rlogout\r\n\x1b]133;D;0;nonce=n\x07\x1b]133;B;nonce=n\x07");
        assert_eq!(
            ups,
            vec![
                TrackerUpdate::RemoteCommandStarted { seq: 2 },
                TrackerUpdate::RemoteCommandFinished { seq: 2, exit_code: None },
                TrackerUpdate::RemoteDisconnected { exit_code: Some(0) },
                TrackerUpdate::State { state: ShellState::Prompt },
            ]
        );
        assert!(t.remote().is_none());
    }

    #[test]
    fn remote_shell_integration_gives_exit_codes() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        start(&mut t, &mut s, "ssh box");
        // 远程 bash 装了集成：发不带 nonce 的 133，同时也开括号粘贴
        let ups = run(&mut t, &mut s, b"\x1b]133;A\x07box$ \x1b]133;B\x07\x1b[?2004h");
        assert!(ups.contains(&TrackerUpdate::RemoteConnected {
            target: ConnectTarget { via: "ssh".into(), host: "box".into(), user: None }
        }));
        let ups = run(&mut t, &mut s, b"false\r\n\x1b[?2004l\x1b]133;C\x07");
        assert_eq!(ups, vec![TrackerUpdate::RemoteCommandStarted { seq: 1 }]);
        let ups = run(&mut t, &mut s, b"\x1b]133;D;1\x07\x1b]133;A\x07box$ \x1b]133;B\x07\x1b[?2004h");
        assert_eq!(
            ups,
            vec![TrackerUpdate::RemoteCommandFinished { seq: 1, exit_code: Some(1) }, TrackerUpdate::RemotePrompt]
        );
    }

    #[test]
    fn failed_connection_is_a_failed_command() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        start(&mut t, &mut s, "ssh nowhere");
        run(&mut t, &mut s, b"ssh: Could not resolve hostname nowhere\r\n");
        let ups = run(&mut t, &mut s, b"\x1b]133;D;255;nonce=n\x07");
        assert_eq!(
            ups,
            vec![TrackerUpdate::CommandFinished { text: Some("ssh nowhere".into()), exit_code: Some(255) }]
        );
        assert!(t.remote().is_none());
    }

    #[test]
    fn old_bash_without_signals_connects_by_prompt_shape() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        start(&mut t, &mut s, "ssh root@centos7");
        assert!(t.observe_output(b"root@centos7's password: ").is_empty());
        assert!(t.observe_output(b"\r\nLast login: Mon\r\n").is_empty());
        let ups = t.observe_output(b"\x1b]0;root@centos7:~\x07[root@centos7 ~]# ");
        assert!(matches!(ups.as_slice(), [TrackerUpdate::RemoteConnected { .. }]));
        assert_eq!(t.remote().map(|r| r.1), Some(true));
    }

    #[test]
    fn plain_commands_are_not_remote() {
        let (mut t, mut s) = (Tracker::default(), Scanner::new(Some("n".into())));
        start(&mut t, &mut s, "ssh box uptime");
        assert!(t.remote().is_none());
        assert!(t.observe_output(b"box$ ").is_empty());
    }
}
