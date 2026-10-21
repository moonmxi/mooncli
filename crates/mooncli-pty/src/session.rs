//! 一个终端会话 = 一个伪终端 + 一个 shell 进程。设计见 docs/02-terminal-core.md 第 1、2、6 节。
//!
//! 线程：
//! - 读线程：阻塞读伪终端输出，交给扫描器拆出 shell 集成事件，送进通道
//! - 攒批线程：每 4ms 或攒满 64KB 交给 [`Sink`] 一批（输出和事件保持原来的先后顺序）
//! - 等待线程：等 shell 退出，记下退出码后关闭伪终端，让读线程结束
//!   （Windows 上 shell 退出后 ConPTY 的输出管道不会自己关闭，必须主动关）

use std::io::{Read, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mooncli_protocol::{Scanner, Segment, ShellEvent};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::{Error, Result};

pub const BATCH_INTERVAL: Duration = Duration::from_millis(4);
pub const BATCH_BYTES: usize = 64 * 1024;
/// 界面积压超过这么多字节没处理完，就暂停读取（docs/02 §2 背压）
pub const HIGH_WATER: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Output(Vec<u8>),
    Event(ShellEvent),
}

/// 会话把输出交给谁。Tauri 主程序里的实现会转发到界面。
pub trait Sink: Send + Sync + 'static {
    fn batch(&self, items: Vec<Item>);
    fn exited(&self, exit_code: Option<u32>);
}

#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// shell 集成用的随机串；`None` 表示不开集成
    pub nonce: Option<String>,
    /// 是否启用背压。启用后界面必须调用 [`Session::ack`]，否则读到 2MB 就会停下等待。
    pub flow_control: bool,
}

/// 背压计数：已发出但界面还没确认处理完的字节数。
struct Flow {
    enabled: bool,
    unacked: Mutex<usize>,
    cv: Condvar,
    closed: Mutex<bool>,
}

impl Flow {
    fn sent(&self, n: usize) {
        if self.enabled {
            *self.unacked.lock().unwrap() += n;
        }
    }

    fn ack(&self, n: usize) {
        let mut u = self.unacked.lock().unwrap();
        *u = u.saturating_sub(n);
        self.cv.notify_all();
    }

    fn wait_below_high_water(&self) {
        if !self.enabled {
            return;
        }
        let mut u = self.unacked.lock().unwrap();
        while *u > HIGH_WATER && !*self.closed.lock().unwrap() {
            u = self.cv.wait_timeout(u, Duration::from_millis(50)).unwrap().0;
        }
    }

    fn close(&self) {
        *self.closed.lock().unwrap() = true;
        self.cv.notify_all();
    }
}

pub struct Session {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    flow: Arc<Flow>,
    pid: Option<u32>,
}

impl Session {
    pub fn spawn(opts: SpawnOptions, sink: Arc<dyn Sink>) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize { rows: opts.rows, cols: opts.cols, pixel_width: 0, pixel_height: 0 })
            .map_err(Error::pty)?;
        let mut cmd = CommandBuilder::new(&opts.program);
        cmd.args(&opts.args);
        if let Some(cwd) = &opts.cwd {
            cmd.cwd(cwd);
        }
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| Error::Spawn(opts.program.clone(), e.to_string()))?;
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(Error::pty)?;
        let writer = pair.master.take_writer().map_err(Error::pty)?;
        let master = Arc::new(Mutex::new(Some(pair.master)));
        let flow = Arc::new(Flow {
            enabled: opts.flow_control,
            unacked: Mutex::new(0),
            cv: Condvar::new(),
            closed: Mutex::new(false),
        });

        let (tx, rx) = mpsc::channel::<Item>();
        let exit_code = Arc::new(Mutex::new(None::<u32>));

        // 读线程
        {
            let flow = flow.clone();
            let nonce = opts.nonce.clone();
            thread::Builder::new().name("pty-read".into()).spawn(move || {
                let mut scanner = Scanner::new(nonce);
                let mut buf = vec![0u8; 16 * 1024];
                loop {
                    flow.wait_below_high_water();
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let mut closed = false;
                    for seg in scanner.feed(&buf[..n]).into_ordered() {
                        let item = match seg {
                            Segment::Bytes(b) => Item::Output(b),
                            Segment::Event(e) => Item::Event(e),
                        };
                        if tx.send(item).is_err() {
                            closed = true;
                            break;
                        }
                    }
                    if closed {
                        break;
                    }
                }
            })?;
        }

        // 等待线程
        {
            let master = master.clone();
            let exit_code = exit_code.clone();
            thread::Builder::new().name("pty-wait".into()).spawn(move || {
                let status = child.wait().ok();
                *exit_code.lock().unwrap() = status.map(|s| s.exit_code());
                // 给读线程一点时间把最后的输出读完，再关掉伪终端
                thread::sleep(Duration::from_millis(100));
                master.lock().unwrap().take();
            })?;
        }

        // 攒批线程
        {
            let flow = flow.clone();
            thread::Builder::new().name("pty-batch".into()).spawn(move || {
                let mut batch: Vec<Item> = Vec::new();
                let mut bytes = 0usize;
                let mut started: Option<Instant> = None;
                let flush = |batch: &mut Vec<Item>, bytes: &mut usize, started: &mut Option<Instant>| {
                    if batch.is_empty() {
                        return;
                    }
                    flow.sent(*bytes);
                    sink.batch(std::mem::take(batch));
                    *bytes = 0;
                    *started = None;
                };
                loop {
                    let timeout = match started {
                        Some(t) => BATCH_INTERVAL.saturating_sub(t.elapsed()),
                        None => Duration::from_secs(3600),
                    };
                    match rx.recv_timeout(timeout) {
                        Ok(item) => {
                            started.get_or_insert_with(Instant::now);
                            match (item, batch.last_mut()) {
                                (Item::Output(b), Some(Item::Output(last))) => {
                                    bytes += b.len();
                                    last.extend_from_slice(&b);
                                }
                                (Item::Output(b), _) => {
                                    bytes += b.len();
                                    batch.push(Item::Output(b));
                                }
                                (ev, _) => batch.push(ev),
                            }
                            if bytes >= BATCH_BYTES {
                                flush(&mut batch, &mut bytes, &mut started);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => flush(&mut batch, &mut bytes, &mut started),
                        Err(RecvTimeoutError::Disconnected) => {
                            flush(&mut batch, &mut bytes, &mut started);
                            break;
                        }
                    }
                }
                flow.close();
                // 读线程结束可能早于等待线程记下退出码，稍等一下
                let deadline = Instant::now() + Duration::from_secs(2);
                let code = loop {
                    if let Some(c) = *exit_code.lock().unwrap() {
                        break Some(c);
                    }
                    if Instant::now() > deadline {
                        break None;
                    }
                    thread::sleep(Duration::from_millis(10));
                };
                sink.exited(code);
            })?;
        }

        Ok(Self { writer: Mutex::new(writer), master, killer: Mutex::new(killer), flow, pid })
    }

    /// 键盘输入、粘贴内容。
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data)?;
        w.flush()?;
        Ok(())
    }

    /// 大段粘贴：分块写入，块之间稍停，避免 ConPTY 或远程端丢字（docs/05 §5）。
    pub fn write_chunked(&self, data: &[u8]) -> Result<()> {
        const CHUNK: usize = 1024;
        if data.len() <= 4 * CHUNK {
            return self.write(data);
        }
        for c in data.chunks(CHUNK) {
            self.write(c)?;
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        if let Some(m) = self.master.lock().unwrap().as_ref() {
            m.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(Error::pty)?;
        }
        Ok(())
    }

    /// 界面处理完 `n` 字节输出后调用。
    pub fn ack(&self, n: usize) {
        self.flow.ack(n);
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn kill(&self) -> Result<()> {
        self.killer.lock().unwrap().kill()?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.killer.lock().unwrap().kill();
        self.flow.close();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Collect {
        items: Mutex<Vec<Item>>,
        batches: Mutex<usize>,
        exit: Mutex<Option<Option<u32>>>,
        cv: Condvar,
    }

    impl Sink for Collect {
        fn batch(&self, items: Vec<Item>) {
            *self.batches.lock().unwrap() += 1;
            self.items.lock().unwrap().extend(items);
            self.cv.notify_all();
        }
        fn exited(&self, code: Option<u32>) {
            *self.exit.lock().unwrap() = Some(code);
            self.cv.notify_all();
        }
    }

    impl Collect {
        fn output(&self) -> String {
            let items = self.items.lock().unwrap();
            let mut s = Vec::new();
            for i in items.iter() {
                if let Item::Output(b) = i {
                    s.extend_from_slice(b);
                }
            }
            String::from_utf8_lossy(&s).into_owned()
        }

        fn wait_exit(&self) -> Option<u32> {
            let mut g = self.exit.lock().unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while g.is_none() {
                assert!(Instant::now() < deadline, "等待退出超时");
                g = self.cv.wait_timeout(g, Duration::from_millis(50)).unwrap().0;
            }
            g.unwrap()
        }

        fn wait_output(&self, needle: &str) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !self.output().contains(needle) {
                assert!(Instant::now() < deadline, "等不到输出 {needle:?}，现在是 {:?}", self.output());
                thread::sleep(Duration::from_millis(20));
            }
        }
    }

    fn sh(script: &str) -> SpawnOptions {
        SpawnOptions {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            cwd: None,
            env: vec![("MOONCLI_TEST".into(), "1".into())],
            cols: 80,
            rows: 24,
            nonce: Some("n".into()),
            flow_control: false,
        }
    }

    #[test]
    fn output_events_and_exit_code() {
        let sink = Arc::new(Collect::default());
        let _s = Session::spawn(sh(r#"printf '\033]133;A;nonce=n\007hello %s' "$MOONCLI_TEST"; exit 3"#), sink.clone())
            .unwrap();
        assert_eq!(sink.wait_exit(), Some(3));
        assert!(sink.output().contains("hello 1"));
        let items = sink.items.lock().unwrap();
        assert!(items.iter().any(|i| matches!(
            i,
            Item::Event(ShellEvent::Mark { kind: mooncli_protocol::MarkKind::PromptStart, trusted: true })
        )));
    }

    #[test]
    fn write_reaches_the_shell() {
        let sink = Arc::new(Collect::default());
        let s = Session::spawn(sh("read x; echo got:$x"), sink.clone()).unwrap();
        s.write(b"abc\r").unwrap();
        sink.wait_output("got:abc");
        assert_eq!(sink.wait_exit(), Some(0));
    }

    #[test]
    fn resize_is_seen_by_child() {
        let sink = Arc::new(Collect::default());
        let s = Session::spawn(sh("read go; stty size"), sink.clone()).unwrap();
        s.resize(100, 40).unwrap();
        s.write(b"\r").unwrap();
        sink.wait_output("40 100");
    }

    #[test]
    fn large_output_is_batched_and_complete() {
        let sink = Arc::new(Collect::default());
        // 约 1MB 输出
        let _s = Session::spawn(sh("i=0; while [ $i -lt 20000 ]; do echo 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; i=$((i+1)); done"), sink.clone()).unwrap();
        sink.wait_exit();
        let out = sink.output();
        assert_eq!(out.matches("xxxx\r\n").count(), 20000);
        let batches = *sink.batches.lock().unwrap();
        assert!(batches < 2000, "批次太碎: {batches}");
    }

    #[test]
    fn flow_control_pauses_until_ack() {
        let sink = Arc::new(Collect::default());
        let mut o = sh("head -c 6000000 /dev/zero | tr '\\0' 'a'");
        o.flow_control = true;
        let s = Session::spawn(o, sink.clone()).unwrap();
        thread::sleep(Duration::from_millis(800));
        let got = sink.output().len();
        // 没确认之前，读到的量不会远超高水位
        assert!(got <= HIGH_WATER + BATCH_BYTES * 2 + 16 * 1024, "背压没生效: {got}");
        assert!(sink.exit.lock().unwrap().is_none());
        // 界面持续确认后能读完
        let deadline = Instant::now() + Duration::from_secs(20);
        while sink.exit.lock().unwrap().is_none() {
            s.ack(usize::MAX / 2);
            assert!(Instant::now() < deadline, "确认后仍然读不完");
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(sink.output().len(), 6_000_000);
    }

    #[test]
    fn kill_ends_session() {
        let sink = Arc::new(Collect::default());
        let s = Session::spawn(sh("sleep 30"), sink.clone()).unwrap();
        s.kill().unwrap();
        sink.wait_exit();
    }
}
