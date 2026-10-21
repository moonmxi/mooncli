//! 会话状态机：决定"现在按键归谁"。规则见 docs/05-interaction.md 第 1 节。
//!
//! 只有带 nonce 的标记（本机 pwsh 集成脚本发的）能把会话切到"提示符上"。
//! ssh 到远程后远程 shell 发的标记不带 nonce，只当作远程信息，不会让 mooncli 开始拦按键。

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
    State { state: ShellState },
    /// 本机命令开始。`text` 来自私有序列 `cmd`，可能拿不到。
    CommandStarted { text: Option<String> },
    CommandFinished { text: Option<String>, exit_code: Option<i32> },
    /// 远程 shell（不带 nonce）的提示符，说明用户此刻在远程的命令行上。
    RemotePrompt,
}

#[derive(Debug)]
pub struct Tracker {
    base: ShellState,
    alt: bool,
    pending_cmd: Option<String>,
    running_cmd: Option<String>,
    integrated: bool,
}

impl Default for Tracker {
    fn default() -> Self {
        Self { base: ShellState::Unknown, alt: false, pending_cmd: None, running_cmd: None, integrated: false }
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

    pub fn apply(&mut self, ev: &ShellEvent) -> Vec<TrackerUpdate> {
        let before = self.state();
        let mut out = Vec::new();
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
                    }
                }
            }
            ShellEvent::CommandEnd { exit_code, trusted: true } => {
                self.integrated = true;
                // D 之后马上会有 A/B，这里先回到 Running 以外的状态由 B 决定
                out.push(TrackerUpdate::CommandFinished { text: self.running_cmd.take(), exit_code: *exit_code });
                // 命令结束时全屏程序肯定也退出了（防止程序崩溃没发 ?1049l）
                self.alt = false;
            }
            ShellEvent::Mark { kind: MarkKind::PromptEnd, trusted: false } => {
                out.push(TrackerUpdate::RemotePrompt);
            }
            ShellEvent::Private { kind, data } if kind == "cmd" => {
                self.pending_cmd = data.get("text").and_then(|t| t.as_str()).map(str::to_string);
            }
            ShellEvent::AltScreen { on } => self.alt = *on,
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

        let ups = run(&mut t, &mut s, b"output\r\n\x1b]133;D;0;nonce=n\x07\x1b]133;A;nonce=n\x07\x1b]133;B;nonce=n\x07");
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
}
