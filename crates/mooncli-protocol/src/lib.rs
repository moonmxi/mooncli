//! mooncli 终端协议：从输出流里识别 shell 集成标记、维护会话状态、清理程序标题。

pub mod scanner;
pub mod title;
pub mod tracker;

pub use scanner::{MarkKind, PRIVATE_OSC, ScanOutput, Scanner, Segment, ShellEvent};
pub use title::{CleanTitle, clean_title, is_uninformative};
pub use tracker::{ShellState, Tracker, TrackerUpdate};
