//! 用真实的 pwsh 跑 shell/pwsh/MoonCli.ps1，检查标记、命令文本、退出码、目录。
//! 机器上没有 pwsh 时跳过（会打印提示）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mooncli_protocol::{MarkKind, ShellEvent, ShellState, Tracker, TrackerUpdate};
use mooncli_pty::shell::{LaunchRequest, ShellKind, find_shell, launch};
use mooncli_pty::{Item, Session, Sink};

#[derive(Default)]
struct Rec {
    out: Mutex<Vec<u8>>,
    events: Mutex<Vec<ShellEvent>>,
    tracker: Mutex<Tracker>,
    updates: Mutex<Vec<TrackerUpdate>>,
    session: Mutex<Option<Arc<Session>>>,
}

impl Sink for Rec {
    fn batch(&self, items: Vec<Item>) {
        let mut n = 0;
        let mut dsr = 0;
        for i in items {
            match i {
                Item::Output(b) => {
                    n += b.len();
                    dsr += b.windows(4).filter(|w| w == b"\x1b[6n").count();
                    self.out.lock().unwrap().extend(b);
                }
                Item::Event(e) => {
                    let ups = self.tracker.lock().unwrap().apply(&e);
                    self.updates.lock().unwrap().extend(ups);
                    self.events.lock().unwrap().push(e);
                }
            }
        }
        if let Some(s) = self.session.lock().unwrap().as_ref() {
            s.ack(n);
            // 像真实终端一样回答光标位置查询，pwsh/.NET 启动和 PSReadLine 渲染都要用
            for _ in 0..dsr {
                let _ = s.write(b"\x1b[1;1R");
            }
        }
    }
    fn exited(&self, _: Option<u32>) {}
}

impl Rec {
    fn wait(&self, what: &str, f: impl Fn(&Rec) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !f(self) {
            if Instant::now() > deadline {
                panic!(
                    "等待超时：{what}\n输出：{}\n更新：{:#?}",
                    String::from_utf8_lossy(&self.out.lock().unwrap()),
                    self.updates.lock().unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    fn prompt_count(&self) -> usize {
        self.updates
            .lock()
            .unwrap()
            .iter()
            .filter(|u| **u == TrackerUpdate::State { state: ShellState::Prompt })
            .count()
    }

    fn finished(&self) -> Vec<(Option<String>, Option<i32>)> {
        self.updates
            .lock()
            .unwrap()
            .iter()
            .filter_map(|u| match u {
                TrackerUpdate::CommandFinished { text, exit_code } => Some((text.clone(), *exit_code)),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn pwsh_integration_script_end_to_end() {
    let Some(shell) = find_shell(None).filter(|s| s.kind == ShellKind::Pwsh) else {
        eprintln!("没找到 pwsh，跳过");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/pwsh/MoonCli.ps1");
    let tmp = std::env::temp_dir();
    let opts = launch(LaunchRequest {
        shell: &shell,
        // -NoProfile：测试不受本机 profile 影响
        extra_args: &["-NoProfile".to_string()],
        integration_script: Some(&script),
        session_id: "it",
        cwd: Some(tmp.display().to_string()),
        cols: 120,
        rows: 30,
        app_version: "test",
    });
    let rec = Arc::new(Rec::default());
    let s = Arc::new(Session::spawn(opts, rec.clone()).unwrap());
    *rec.session.lock().unwrap() = Some(s.clone());

    rec.wait("第一个提示符", |r| r.prompt_count() >= 1);
    assert!(rec.tracker.lock().unwrap().is_integrated());
    // 会话信息
    rec.wait("env 私有序列", |r| {
        r.events.lock().unwrap().iter().any(|e| matches!(e, ShellEvent::Private { kind, .. } if kind == "env"))
    });
    // 私有序列不应该出现在显示的输出里
    assert!(!String::from_utf8_lossy(&rec.out.lock().unwrap()).contains("6973"));

    // 成功的命令
    s.write(b"Write-Output 'hi there'\r").unwrap();
    rec.wait("第 1 条命令结束", |r| !r.finished().is_empty());
    assert_eq!(rec.finished()[0], (Some("Write-Output 'hi there'".into()), Some(0)));

    // 原生程序非 0 退出
    // 用 pwsh 自己产生非 0 退出码，Windows 和 Linux 都能跑
    let native = format!("& '{}' -NoProfile -NonInteractive -Command 'exit 3'\r", shell.program.display());
    s.write(native.as_bytes()).unwrap();
    rec.wait("第 2 条命令结束", |r| r.finished().len() >= 2);
    assert_eq!(rec.finished()[1].1, Some(3));

    // cmdlet 出错
    s.write(b"Get-Item /definitely/not/here\r").unwrap();
    rec.wait("第 3 条命令结束", |r| r.finished().len() >= 3);
    assert_eq!(rec.finished()[2].1, Some(1));

    // cmdlet 出错，但 $LASTEXITCODE 还是上一次原生命令留下的 3：不应误报成 3
    s.write(b"Get-Item /still/not/here\r").unwrap();
    rec.wait("第 4 条命令结束", |r| r.finished().len() >= 4);
    assert_eq!(rec.finished()[3].1, Some(1));

    // 空回车不算命令：会画一个新提示符，但没有命令开始/结束
    let prompts = |r: &Rec| {
        r.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, ShellEvent::Mark { kind: MarkKind::PromptStart, trusted: true }))
            .count()
    };
    let before = prompts(&rec);
    s.write(b"\r").unwrap();
    rec.wait("空回车后的提示符", |r| prompts(r) > before);
    assert_eq!(rec.finished().len(), 4);

    // 切换目录
    let target = std::env::temp_dir().join("mooncli-it-dir");
    std::fs::create_dir_all(&target).unwrap();
    s.write(format!("Set-Location '{}'\r", target.display()).as_bytes()).unwrap();
    rec.wait("目录变成临时目录", |r| {
        r.events.lock().unwrap().iter().any(|e| {
            matches!(e, ShellEvent::Cwd { host: Some(_), path }
                if path.replace('\\', "/").to_lowercase().trim_end_matches('/').ends_with("mooncli-it-dir"))
        })
    });

    // 多行命令完整拿到
    s.write(b"if ($true) {\r").unwrap();
    s.write(b"  'multi'\r").unwrap();
    s.write(b"}\r").unwrap();
    rec.wait("多行命令结束", |r| r.finished().len() >= 6);
    let text = rec.finished()[5].0.clone().unwrap();
    assert!(text.contains("if ($true) {") && text.contains("'multi'") && text.trim_end().ends_with('}'), "{text:?}");

    // 补全请求：输入一半，发送 F12 F11
    s.write(b"Get-ChildI").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    s.write(b"\x1b[24~\x1b[23~").unwrap();
    rec.wait("补全结果", |r| {
        r.events.lock().unwrap().iter().any(|e| matches!(e, ShellEvent::Private { kind, .. } if kind == "completions"))
    });
    let data = rec
        .events
        .lock()
        .unwrap()
        .iter()
        .find_map(|e| match e {
            ShellEvent::Private { kind, data } if kind == "completions" => Some(data.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(data["line"], "Get-ChildI");
    assert_eq!(data["start"], 0);
    assert_eq!(data["len"], 10);
    let items: Vec<String> =
        data["items"].as_array().unwrap().iter().map(|i| i["t"].as_str().unwrap().to_string()).collect();
    assert!(items.contains(&"Get-ChildItem".to_string()), "{items:?}");
    // 补全请求不应该改动输入行，也不应该算作执行了命令
    assert_eq!(rec.finished().len(), 6);
    assert_eq!(rec.tracker.lock().unwrap().state(), ShellState::Prompt);

    s.write(b"\x03exit\r").unwrap();
}
