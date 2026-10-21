//! 从 ConPTY 输出流里找出 shell 集成标记。
//!
//! 输出原样转发给 xterm.js，只有 mooncli 私有序列（OSC 6973）会被拿掉。
//! 序列可能被切在两次读取之间，所以扫描器是一个跨调用保留状态的小状态机。
//! 协议说明见 docs/02-terminal-core.md 第 4 节。

use base64::Engine;

/// mooncli 私有 OSC 编号。
pub const PRIVATE_OSC: &str = "6973";

/// 单个 OSC 最多缓存这么多字节，超过就放弃解析、原样转发，防止内存被撑爆。
const MAX_OSC_LEN: usize = 8 * 1024 * 1024;
const MAX_CSI_PARAMS: usize = 64;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShellEvent {
    /// OSC 133 A/B/C/D。`trusted` 表示带了本会话的 nonce，来自本机 pwsh 集成脚本。
    Mark {
        kind: MarkKind,
        trusted: bool,
    },
    CommandEnd {
        exit_code: Option<i32>,
        trusted: bool,
    },
    Cwd {
        host: Option<String>,
        path: String,
    },
    Title {
        text: String,
    },
    Bell,
    Notify {
        title: Option<String>,
        body: String,
    },
    /// OSC 9;4 进度（Windows Terminal 支持的写法）。state: 0 清除 1 正常 2 出错 3 不确定 4 暂停
    Progress {
        state: u8,
        value: u8,
    },
    AltScreen {
        on: bool,
    },
    /// 带对 nonce 的私有序列，`data` 是解码后的 JSON。
    Private {
        kind: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkKind {
    PromptStart,
    PromptEnd,
    CommandStart,
}

#[derive(Debug, Default)]
pub struct ScanOutput {
    /// 要交给 xterm.js 显示的字节。
    pub passthrough: Vec<u8>,
    pub events: Vec<ShellEvent>,
    /// 每个事件发生时 `passthrough` 的长度，用来把事件插回输出流里原来的位置
    pub positions: Vec<usize>,
}

impl ScanOutput {
    fn emit(&mut self, ev: ShellEvent) {
        self.positions.push(self.passthrough.len());
        self.events.push(ev);
    }

    /// 按原来的先后顺序交替给出输出片段和事件。
    pub fn into_ordered(self) -> Vec<Segment> {
        let mut out = Vec::with_capacity(self.events.len() * 2 + 1);
        let mut bytes = self.passthrough;
        let mut taken = 0;
        let mut rest: Vec<u8>;
        for (ev, pos) in self.events.into_iter().zip(self.positions) {
            if pos > taken {
                rest = bytes.split_off(pos - taken);
                out.push(Segment::Bytes(std::mem::replace(&mut bytes, rest)));
                taken = pos;
            }
            out.push(Segment::Event(ev));
        }
        if !bytes.is_empty() {
            out.push(Segment::Bytes(bytes));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Bytes(Vec<u8>),
    Event(ShellEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

pub struct Scanner {
    nonce: Option<String>,
    state: State,
    osc: Vec<u8>,
    csi: Vec<u8>,
    osc_overflow: bool,
}

impl Scanner {
    /// `nonce` 是启动会话时生成并通过环境变量交给集成脚本的随机串。
    pub fn new(nonce: Option<String>) -> Self {
        Self { nonce, state: State::Ground, osc: Vec::new(), csi: Vec::new(), osc_overflow: false }
    }

    pub fn feed(&mut self, input: &[u8]) -> ScanOutput {
        let mut out = ScanOutput { passthrough: Vec::with_capacity(input.len()), ..Default::default() };
        for &b in input {
            self.step(b, &mut out);
        }
        out
    }

    fn step(&mut self, b: u8, out: &mut ScanOutput) {
        match self.state {
            State::Ground => match b {
                0x1b => self.state = State::Esc,
                0x07 => {
                    out.emit(ShellEvent::Bell);
                    out.passthrough.push(b);
                }
                _ => out.passthrough.push(b),
            },
            State::Esc => match b {
                b']' => {
                    self.state = State::Osc;
                    self.osc.clear();
                    self.osc_overflow = false;
                }
                b'[' => {
                    self.state = State::Csi;
                    self.csi.clear();
                    out.passthrough.extend_from_slice(b"\x1b[");
                }
                0x1b => out.passthrough.push(0x1b),
                _ => {
                    out.passthrough.extend_from_slice(&[0x1b, b]);
                    self.state = State::Ground;
                }
            },
            State::Csi => {
                out.passthrough.push(b);
                match b {
                    0x30..=0x3f => {
                        if self.csi.len() < MAX_CSI_PARAMS {
                            self.csi.push(b);
                        }
                    }
                    0x20..=0x2f => {}
                    0x40..=0x7e => {
                        self.finish_csi(b, out);
                        self.state = State::Ground;
                    }
                    // CSI 里出现控制字符：按 VT 规范照常执行，不打断序列
                    _ => {}
                }
            }
            State::Osc => match b {
                0x07 => {
                    self.finish_osc(b"\x07", out);
                    self.state = State::Ground;
                }
                0x1b => self.state = State::OscEsc,
                _ => self.push_osc(b, out),
            },
            State::OscEsc => {
                if b == b'\\' {
                    self.finish_osc(b"\x1b\\", out);
                    self.state = State::Ground;
                } else {
                    // 没有 ST 就开始了新的转义序列：当前 OSC 视为结束，再按 ESC 后的字节处理
                    self.finish_osc(b"", out);
                    self.state = State::Esc;
                    self.step(b, out);
                }
            }
        }
    }

    fn push_osc(&mut self, b: u8, out: &mut ScanOutput) {
        if self.osc_overflow {
            out.passthrough.push(b);
            return;
        }
        self.osc.push(b);
        if self.osc.len() > MAX_OSC_LEN {
            self.osc_overflow = true;
            out.passthrough.extend_from_slice(b"\x1b]");
            out.passthrough.append(&mut self.osc);
        }
    }

    fn finish_csi(&mut self, fin: u8, out: &mut ScanOutput) {
        if fin != b'h' && fin != b'l' {
            return;
        }
        let Some(params) = self.csi.strip_prefix(b"?") else { return };
        let on = fin == b'h';
        let alt = params.split(|&c| c == b';').any(|p| matches!(p, b"1049" | b"1047" | b"47"));
        if alt {
            out.emit(ShellEvent::AltScreen { on });
        }
    }

    fn finish_osc(&mut self, terminator: &[u8], out: &mut ScanOutput) {
        if self.osc_overflow {
            out.passthrough.extend_from_slice(terminator);
            self.osc_overflow = false;
            return;
        }
        let body = std::mem::take(&mut self.osc);
        let text = String::from_utf8_lossy(&body);
        let (code, rest) = text.split_once(';').unwrap_or((&text, ""));
        if code == PRIVATE_OSC {
            if let Some(ev) = self.parse_private(rest) {
                out.emit(ev);
            }
            return; // 私有序列一律不显示
        }
        if let Some(ev) = self.parse_standard(code, rest) {
            out.emit(ev);
        }
        out.passthrough.extend_from_slice(b"\x1b]");
        out.passthrough.extend_from_slice(&body);
        out.passthrough.extend_from_slice(terminator);
    }

    fn trusted(&self, params: &[&str]) -> bool {
        let Some(nonce) = self.nonce.as_deref() else { return false };
        params.iter().any(|p| p.strip_prefix("nonce=") == Some(nonce))
    }

    fn parse_standard(&self, code: &str, rest: &str) -> Option<ShellEvent> {
        match code {
            "133" => {
                let params: Vec<&str> = rest.split(';').collect();
                let trusted = self.trusted(&params);
                let mark = |kind| Some(ShellEvent::Mark { kind, trusted });
                match params[0] {
                    "A" => mark(MarkKind::PromptStart),
                    "B" => mark(MarkKind::PromptEnd),
                    "C" => mark(MarkKind::CommandStart),
                    "D" => {
                        let exit_code = params.get(1).and_then(|p| p.parse().ok());
                        Some(ShellEvent::CommandEnd { exit_code, trusted })
                    }
                    _ => None,
                }
            }
            "7" => parse_file_url(rest),
            "9" => {
                let (sub, arg) = rest.split_once(';').unwrap_or((rest, ""));
                match sub {
                    "9" => Some(ShellEvent::Cwd { host: None, path: arg.trim_matches('"').to_string() }),
                    "4" => {
                        let mut it = arg.split(';');
                        let state = it.next()?.parse().ok()?;
                        let value = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                        Some(ShellEvent::Progress { state, value })
                    }
                    // ConEmu 的 9;1 到 9;12 是各种控制命令，不是通知
                    s if s.parse::<u8>().is_ok_and(|n| (1..=12).contains(&n)) => None,
                    _ if rest.is_empty() => None,
                    _ => Some(ShellEvent::Notify { title: None, body: rest.to_string() }),
                }
            }
            "777" => {
                let mut it = rest.splitn(3, ';');
                if it.next()? != "notify" {
                    return None;
                }
                let title = it.next().map(str::to_string);
                let body = it.next().unwrap_or_default().to_string();
                Some(ShellEvent::Notify { title, body })
            }
            "0" | "2" => Some(ShellEvent::Title { text: rest.to_string() }),
            _ => None,
        }
    }

    fn parse_private(&self, rest: &str) -> Option<ShellEvent> {
        let mut it = rest.splitn(3, ';');
        let nonce = it.next()?;
        if self.nonce.as_deref() != Some(nonce) {
            return None;
        }
        let kind = it.next()?.to_string();
        let payload = it.next().unwrap_or_default();
        let data = if payload.is_empty() {
            serde_json::Value::Null
        } else {
            let bytes = base64::engine::general_purpose::STANDARD.decode(payload).ok()?;
            serde_json::from_slice(&bytes).ok()?
        };
        Some(ShellEvent::Private { kind, data })
    }
}

/// 解析 `file://host/path`。Windows 路径会写成 `file://host/C:/Users/...`。
fn parse_file_url(s: &str) -> Option<ShellEvent> {
    let rest = s.strip_prefix("file://")?;
    let (host, path) = {
        let i = rest.find('/')?;
        (&rest[..i], &rest[i..])
    };
    let mut path = percent_decode(path);
    // "/C:/x" -> "C:/x"
    let b = path.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        path.remove(0);
    }
    let host = (!host.is_empty()).then(|| host.to_string());
    Some(ShellEvent::Cwd { host, path })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(nonce: Option<&str>, chunks: &[&[u8]]) -> ScanOutput {
        let mut s = Scanner::new(nonce.map(str::to_string));
        let mut all = ScanOutput::default();
        for c in chunks {
            let mut o = s.feed(c);
            all.passthrough.append(&mut o.passthrough);
            all.events.append(&mut o.events);
            all.positions.append(&mut o.positions);
        }
        all
    }

    #[test]
    fn ordered_segments_keep_event_positions() {
        let mut s = Scanner::new(Some("n".into()));
        let segs = s.feed(b"out1\x1b]133;D;0;nonce=n\x07\x1b]6973;n;env;\x07out2\x07").into_ordered();
        assert_eq!(
            segs,
            vec![
                Segment::Bytes(b"out1".to_vec()),
                Segment::Event(ShellEvent::CommandEnd { exit_code: Some(0), trusted: true }),
                Segment::Bytes(b"\x1b]133;D;0;nonce=n\x07".to_vec()),
                Segment::Event(ShellEvent::Private { kind: "env".into(), data: serde_json::Value::Null }),
                Segment::Bytes(b"out2".to_vec()),
                Segment::Event(ShellEvent::Bell),
                Segment::Bytes(b"\x07".to_vec()),
            ]
        );
    }

    #[test]
    fn plain_text_passes_through() {
        let o = scan(None, &[b"hello \x1b[31mred\x1b[0m\r\n"]);
        assert_eq!(o.passthrough, b"hello \x1b[31mred\x1b[0m\r\n");
        assert!(o.events.is_empty());
    }

    #[test]
    fn utf8_bytes_are_not_mistaken_for_c1() {
        // "这" = E8 BF 99，"拝" 含 0x9D
        let text = "这是中文 拝".as_bytes();
        let o = scan(None, &[text]);
        assert_eq!(o.passthrough, text);
    }

    #[test]
    fn marks_trusted_only_with_nonce() {
        let o = scan(Some("abc"), &[b"\x1b]133;A;nonce=abc\x07\x1b]133;B\x07"]);
        assert_eq!(
            o.events,
            vec![
                ShellEvent::Mark { kind: MarkKind::PromptStart, trusted: true },
                ShellEvent::Mark { kind: MarkKind::PromptEnd, trusted: false },
            ]
        );
        // 标准序列仍然转发，xterm.js 自己会忽略
        assert_eq!(o.passthrough, b"\x1b]133;A;nonce=abc\x07\x1b]133;B\x07");
    }

    #[test]
    fn command_end_with_exit_code_and_st_terminator() {
        let o = scan(Some("n"), &[b"\x1b]133;D;3;nonce=n\x1b\\"]);
        assert_eq!(o.events, vec![ShellEvent::CommandEnd { exit_code: Some(3), trusted: true }]);
    }

    #[test]
    fn sequence_split_across_chunks() {
        let o = scan(Some("n"), &[b"ab\x1b", b"]13", b"3;C;nonce", b"=n\x1b", b"\\cd"]);
        assert_eq!(o.events, vec![ShellEvent::Mark { kind: MarkKind::CommandStart, trusted: true }]);
        assert_eq!(o.passthrough, b"ab\x1b]133;C;nonce=n\x1b\\cd");
    }

    #[test]
    fn private_sequence_is_stripped_and_decoded() {
        let payload = base64::engine::general_purpose::STANDARD.encode(r#"{"text":"git status"}"#);
        let seq = format!("x\x1b]6973;n1;cmd;{payload}\x07y");
        let o = scan(Some("n1"), &[seq.as_bytes()]);
        assert_eq!(o.passthrough, b"xy");
        assert_eq!(
            o.events,
            vec![ShellEvent::Private { kind: "cmd".into(), data: serde_json::json!({"text": "git status"}) }]
        );
    }

    #[test]
    fn private_sequence_with_wrong_nonce_is_dropped_silently() {
        // 远程服务器上的程序伪造私有序列：不显示，也不产生事件
        let o = scan(Some("real"), &[b"\x1b]6973;fake;cmd;e30=\x07"]);
        assert!(o.passthrough.is_empty());
        assert!(o.events.is_empty());
    }

    #[test]
    fn cwd_from_osc7_windows_path() {
        let o = scan(None, &[b"\x1b]7;file://MOON-PC/C:/Users/moon/My%20Code\x07"]);
        assert_eq!(
            o.events,
            vec![ShellEvent::Cwd { host: Some("MOON-PC".into()), path: "C:/Users/moon/My Code".into() }]
        );
    }

    #[test]
    fn cwd_from_osc9_9() {
        let o = scan(None, &[b"\x1b]9;9;\"C:\\Users\\moon\"\x07"]);
        assert_eq!(o.events, vec![ShellEvent::Cwd { host: None, path: "C:\\Users\\moon".into() }]);
    }

    #[test]
    fn title_bell_notify_progress() {
        let o = scan(
            None,
            &[b"\x1b]0;\xe2\x9c\xb3 Fix tabs\x07\x07\x1b]9;Build done\x07\x1b]777;notify;Claude;Waiting\x07\x1b]9;4;1;40\x07"],
        );
        assert_eq!(
            o.events,
            vec![
                ShellEvent::Title { text: "✳ Fix tabs".into() },
                ShellEvent::Bell,
                ShellEvent::Notify { title: None, body: "Build done".into() },
                ShellEvent::Notify { title: Some("Claude".into()), body: "Waiting".into() },
                ShellEvent::Progress { state: 1, value: 40 },
            ]
        );
    }

    #[test]
    fn bel_inside_osc_is_terminator_not_bell() {
        let o = scan(None, &[b"\x1b]2;t\x07"]);
        assert_eq!(o.events, vec![ShellEvent::Title { text: "t".into() }]);
    }

    #[test]
    fn alt_screen_enter_and_leave() {
        let o = scan(None, &[b"\x1b[?1049h", b"vim", b"\x1b[?25;1049l"]);
        assert_eq!(o.events, vec![ShellEvent::AltScreen { on: true }, ShellEvent::AltScreen { on: false }]);
        assert_eq!(o.passthrough, b"\x1b[?1049hvim\x1b[?25;1049l");
    }

    #[test]
    fn osc_interrupted_by_new_escape() {
        let o = scan(None, &[b"\x1b]2;abc\x1b[0m"]);
        assert_eq!(o.events, vec![ShellEvent::Title { text: "abc".into() }]);
        assert_eq!(o.passthrough, b"\x1b]2;abc\x1b[0m");
    }
}
