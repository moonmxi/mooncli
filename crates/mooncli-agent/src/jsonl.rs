//! 读 JSONL 会话文件的尾部：会话文件可能几十 MB，只读最后一段找最新事件，
//! 找不到再扩大范围。返回的是完整行（第一行可能被截断，调用方解析失败自然会跳过）。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 从文件尾部读至多 `max_bytes` 字节，按行切分（顺序保持文件中的顺序）。
pub fn tail_lines(path: &Path, max_bytes: u64) -> Vec<String> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    let start = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::with_capacity((len - start) as usize);
    if file.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines().map(|l| l.to_string()).collect()
}

/// 读文件的第一行（session_meta 之类只在开头的信息）。
pub fn first_line(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let n = file.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            buf.truncate(pos);
            break;
        }
        if buf.len() > 1024 * 1024 {
            break; // 第一行不该这么长，防异常文件
        }
    }
    if buf.is_empty() { None } else { Some(String::from_utf8_lossy(&buf).into_owned()) }
}

/// 递归收集目录下所有 `.jsonl` 文件（`.jsonl.zst` 压缩文件 v1 先跳过）。
pub fn collect_jsonl(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}
