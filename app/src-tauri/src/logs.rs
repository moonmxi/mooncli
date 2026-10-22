//! 崩溃与错误日志（docs/09 M7）：只存在本机的 data/logs 里，不上传。
//! 用户反馈问题时，设置 → 关于里一键把日志和诊断信息打成一个 zip，自己决定发不发。

use std::io::Write;
use std::path::{Path, PathBuf};

/// 日志文件最多留多少个（按时间删最旧的）
const KEEP: usize = 30;

pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    secs.to_string()
}

/// 追加一条到今天的 mooncli.log（界面的错误、主进程的警告）
pub fn append(data_dir: &Path, level: &str, message: &str) {
    let dir = logs_dir(data_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("mooncli.log");
    // 单个文件超过 1MB 就换一个新的，旧的改名留着
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let _ = std::fs::rename(&path, dir.join(format!("mooncli-{}.log", stamp())));
        prune(&dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}] {level}: {message}", stamp());
    }
}

fn prune(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    if files.len() <= KEEP {
        return;
    }
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    for p in &files[..files.len() - KEEP] {
        let _ = std::fs::remove_file(p);
    }
}

/// 主进程崩溃时把出错位置和调用栈写进 crash-<时间>.log，然后照常退出
pub fn install_panic_hook(data_dir: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = logs_dir(&data_dir);
        if std::fs::create_dir_all(&dir).is_ok() {
            let bt = std::backtrace::Backtrace::force_capture();
            let text = format!(
                "mooncli {} 崩溃\n系统：{} {}\n{info}\n\n调用栈：\n{bt}\n",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            let _ = std::fs::write(dir.join(format!("crash-{}.log", stamp())), text);
            prune(&dir);
        }
        default(info);
    }));
}

/// 把日志目录和一份诊断信息打包成 zip，放到导出目录，返回路径
/// label 是界面给的本地时间（20261022-1530），只留数字和连字符
pub fn pack(
    data_dir: &Path,
    export_dir: &Path,
    diagnostics: &serde_json::Value,
    label: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(export_dir).map_err(|e| format!("创建 {} 失败：{e}", export_dir.display()))?;
    let label: String = label.chars().filter(|c| c.is_ascii_digit() || *c == '-').take(20).collect();
    let label = if label.is_empty() { stamp() } else { label };
    let path = export_dir.join(format!("mooncli-logs-{label}.zip"));
    let file = std::fs::File::create(&path).map_err(|e| format!("创建 {} 失败：{e}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let diag = serde_json::to_string_pretty(diagnostics).unwrap_or_default();
    zip.start_file("diagnostics.json", opts).map_err(|e| e.to_string())?;
    zip.write_all(diag.as_bytes()).map_err(|e| e.to_string())?;
    if let Ok(rd) = std::fs::read_dir(logs_dir(data_dir)) {
        for e in rd.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            if let Ok(bytes) = std::fs::read(&p) {
                zip.start_file(format!("logs/{name}"), opts).map_err(|e| e.to_string())?;
                zip.write_all(&bytes).map_err(|e| e.to_string())?;
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_pack() {
        let t = tempfile::tempdir().unwrap();
        append(t.path(), "error", "界面出错：TypeError");
        let text = std::fs::read_to_string(logs_dir(t.path()).join("mooncli.log")).unwrap();
        assert!(text.contains("error: 界面出错"));
        let out = pack(t.path(), &t.path().join("out"), &serde_json::json!({ "appVersion": "0.1.1" }), "20261022-1530")
            .unwrap();
        assert!(out.ends_with("mooncli-logs-20261022-1530.zip"));
        let mut z = zip::ZipArchive::new(std::fs::File::open(out).unwrap()).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect();
        assert_eq!(names, vec!["diagnostics.json", "logs/mooncli.log"]);
    }
}
