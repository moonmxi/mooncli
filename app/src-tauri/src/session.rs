//! 会话桥：把伪终端的输出和 shell 集成事件翻译成界面消息（app/src/backend/types.ts 的 SessionMsg），
//! 顺便把命令写进历史库。

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use mooncli_history::{CommandRecord, Store};
use mooncli_protocol::{ShellEvent, ShellState, Tracker, TrackerUpdate, clean_title, is_uninformative};
use mooncli_pty::{Item, Sink};
use serde::Serialize;
use tauri::ipc::Channel;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UiMsg {
    Output { data: String, bytes: usize },
    State { state: ShellState },
    CommandStarted { text: Option<String> },
    CommandFinished { text: Option<String>, exit_code: Option<i32> },
    RemotePrompt,
    Cwd { host: Option<String>, path: String },
    Title { text: String, activity: Option<String>, informative: bool },
    Bell,
    Notify { title: Option<String>, body: String },
    Progress { state: u8, value: u8 },
    Env { pwsh: Option<String>, host: Option<String>, admin: Option<bool> },
    Completions { data: serde_json::Value },
    Exited { code: Option<u32> },
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

struct Running {
    record_id: Option<i64>,
    started_at: i64,
}

#[derive(Default)]
struct SessionInfo {
    cwd: Option<String>,
    host: Option<String>,
    running: Option<Running>,
}

pub struct UiSink {
    pub id: String,
    channel: Channel<UiMsg>,
    tracker: Mutex<Tracker>,
    info: Mutex<SessionInfo>,
    history: Arc<Mutex<Option<Store>>>,
}

impl UiSink {
    pub fn new(id: String, channel: Channel<UiMsg>, history: Arc<Mutex<Option<Store>>>) -> Self {
        Self { id, channel, tracker: Mutex::new(Tracker::default()), info: Mutex::default(), history }
    }

    fn send(&self, m: UiMsg) {
        // 界面关掉了就算了
        let _ = self.channel.send(m);
    }

    fn on_event(&self, e: ShellEvent) {
        let updates = self.tracker.lock().unwrap().apply(&e);
        for u in updates {
            self.on_update(u);
        }
        match e {
            ShellEvent::Cwd { host, path } => {
                if host.is_none() || self.tracker.lock().unwrap().state() == ShellState::Prompt {
                    self.info.lock().unwrap().cwd = Some(path.clone());
                }
                self.send(UiMsg::Cwd { host, path });
            }
            ShellEvent::Title { text } => {
                let t = clean_title(&text);
                let informative = !is_uninformative(&t.text);
                self.send(UiMsg::Title { text: t.text, activity: t.activity, informative });
            }
            ShellEvent::Bell => self.send(UiMsg::Bell),
            ShellEvent::Notify { title, body } => self.send(UiMsg::Notify { title, body }),
            ShellEvent::Progress { state, value } => self.send(UiMsg::Progress { state, value }),
            ShellEvent::Private { kind, data } => match kind.as_str() {
                "env" => {
                    let s = |k: &str| data.get(k).and_then(|v| v.as_str()).map(str::to_string);
                    let host = s("host");
                    self.info.lock().unwrap().host = host.clone();
                    self.send(UiMsg::Env { pwsh: s("pwsh"), host, admin: data.get("admin").and_then(|v| v.as_bool()) });
                }
                "completions" => self.send(UiMsg::Completions { data }),
                _ => {}
            },
            // 提示符、命令、全屏程序这些由状态机处理
            ShellEvent::Mark { .. } | ShellEvent::CommandEnd { .. } | ShellEvent::AltScreen { .. } => {}
        }
    }

    fn on_update(&self, u: TrackerUpdate) {
        match u {
            TrackerUpdate::State { state } => self.send(UiMsg::State { state }),
            TrackerUpdate::CommandStarted { text } => {
                let started_at = now_ms();
                let record_id = text.as_deref().and_then(|t| self.record_start(t, started_at));
                self.info.lock().unwrap().running = Some(Running { record_id, started_at });
                self.send(UiMsg::CommandStarted { text });
            }
            TrackerUpdate::CommandFinished { text, exit_code } => {
                if let Some(r) = self.info.lock().unwrap().running.take()
                    && let (Some(id), Some(store)) = (r.record_id, self.history.lock().unwrap().as_ref())
                {
                    let _ = store.finish(id, exit_code, now_ms() - r.started_at);
                }
                self.send(UiMsg::CommandFinished { text, exit_code });
            }
            TrackerUpdate::RemotePrompt => self.send(UiMsg::RemotePrompt),
        }
    }

    fn record_start(&self, text: &str, started_at: i64) -> Option<i64> {
        let info = self.info.lock().unwrap();
        let mut rec = CommandRecord::new(text, started_at);
        rec.cwd = info.cwd.clone();
        rec.host = info.host.clone();
        rec.session_id = Some(self.id.clone());
        rec.shell = Some("pwsh".into());
        drop(info);
        self.history.lock().unwrap().as_ref().and_then(|s| s.insert(&rec).ok().flatten())
    }
}

impl Sink for UiSink {
    fn batch(&self, items: Vec<Item>) {
        for item in items {
            match item {
                Item::Output(b) => {
                    let bytes = b.len();
                    let data = base64::engine::general_purpose::STANDARD.encode(&b);
                    self.send(UiMsg::Output { data, bytes });
                }
                Item::Event(e) => self.on_event(e),
            }
        }
    }

    fn exited(&self, code: Option<u32>) {
        self.send(UiMsg::Exited { code });
    }
}
