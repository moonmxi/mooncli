//! 用户数据的管理：设置的自动备份和恢复、整体导出导入、占用空间（docs/03 §7）。
//!
//! 数据放在两个地方：
//! - 配置目录：settings.json、backgrounds/（导入的背景图）、backups/（设置的历次备份）
//! - 本地数据目录：history.db（命令历史）

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::files;

/// 设置备份最多留这么多份，多了删最旧的
const KEEP_BACKUPS: usize = 20;
/// 自动备份的间隔：一天一次
const AUTO_BACKUP_EVERY_MS: i64 = 24 * 60 * 60 * 1000;
/// 导入的备份文件最大多少
const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;

pub struct DataPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub settings: PathBuf,
    pub backgrounds: PathBuf,
    pub backups: PathBuf,
    pub history: PathBuf,
    /// 导出的备份放在哪（下载文件夹，没有就用主目录）
    pub export_dir: PathBuf,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 20261022-153000 这样的本地时间不好算（不想为此加时区库），文件名里直接用毫秒时间戳，界面负责显示成人看的时间
fn stamp() -> String {
    now_ms().to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub at: i64,
    pub bytes: u64,
    /// auto（每天自动）、manual（手动）、before-import、before-restore、before-reset
    pub reason: String,
}

/// 设置备份的文件名：settings-<毫秒>-<原因>.json
fn parse_backup_name(name: &str) -> Option<(i64, String)> {
    let rest = name.strip_prefix("settings-")?.strip_suffix(".json")?;
    let (at, reason) = rest.split_once('-')?;
    let ok = reason.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let at = at.parse().ok()?;
    ok.then(|| (at, reason.to_string()))
}

pub fn list_backups(p: &DataPaths) -> Vec<BackupInfo> {
    let mut out: Vec<BackupInfo> = std::fs::read_dir(&p.backups)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let (at, reason) = parse_backup_name(&name)?;
            Some(BackupInfo { bytes: e.metadata().map(|m| m.len()).unwrap_or(0), name, at, reason })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.at));
    out
}

/// 把当前的 settings.json 存一份。没有设置文件（从没改过设置）时什么都不做
pub fn snapshot_settings(p: &DataPaths, reason: &str) -> std::io::Result<Option<String>> {
    if !p.settings.is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(&p.backups)?;
    let name = format!("settings-{}-{reason}.json", stamp());
    std::fs::copy(&p.settings, p.backups.join(&name))?;
    for old in list_backups(p).into_iter().skip(KEEP_BACKUPS) {
        let _ = std::fs::remove_file(p.backups.join(old.name));
    }
    Ok(Some(name))
}

/// 启动时和每次保存设置时调用：距上次自动备份超过一天就备份一次
pub fn auto_snapshot(p: &DataPaths) {
    let last = list_backups(p).into_iter().find(|b| b.reason == "auto").map(|b| b.at).unwrap_or(0);
    if now_ms() - last > AUTO_BACKUP_EVERY_MS {
        let _ = snapshot_settings(p, "auto");
    }
}

/// 读一份备份的内容（恢复前先给界面看、也用于恢复）
pub fn read_backup(p: &DataPaths, name: &str) -> Result<serde_json::Value, String> {
    parse_backup_name(name).ok_or("备份文件名不对")?;
    let text = std::fs::read_to_string(p.backups.join(name)).map_err(|_| "找不到这份备份")?;
    let v: serde_json::Value = mooncli_config::jsonc::parse(name, &text).map_err(|e| format!("备份内容损坏：{e}"))?;
    if !v.is_object() {
        return Err("备份内容损坏".into());
    }
    Ok(v)
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundInfo {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub config_dir: String,
    pub data_dir: String,
    pub export_dir: String,
    pub settings_bytes: u64,
    pub backgrounds: Vec<BackgroundInfo>,
    pub backups: Vec<BackupInfo>,
    pub backups_bytes: u64,
    pub history_bytes: u64,
    pub history_count: Option<i64>,
}

pub fn overview(p: &DataPaths, history_count: Option<i64>) -> Overview {
    let backgrounds = files::list_backgrounds(&p.backgrounds)
        .into_iter()
        .map(|name| BackgroundInfo { bytes: file_size(&p.backgrounds.join(&name)), name })
        .collect();
    // SQLite 的 WAL 文件也算进去
    let wal = p.history.with_extension("db-wal");
    Overview {
        config_dir: p.config_dir.display().to_string(),
        data_dir: p.data_dir.display().to_string(),
        export_dir: p.export_dir.display().to_string(),
        settings_bytes: file_size(&p.settings),
        backgrounds,
        backups: list_backups(p),
        backups_bytes: dir_size(&p.backups),
        history_bytes: file_size(&p.history) + file_size(&wal),
        history_count,
    }
}

// ---- 导出、导入 ----------------------------------------------------------------

/// 备份包里的说明文件，导入时用来确认这是 mooncli 的备份
#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    app: String,
    version: String,
    created_at: i64,
    has_history: bool,
}

/// 打一个备份包：设置、导入的背景图，可选命令历史（`history_copy` 是已经复制好的历史库文件）。返回包的路径。
/// `label` 是界面给的本地时间（20261022-1530），用在文件名里；不合规就用时间戳
pub fn export(p: &DataPaths, history_copy: Option<&Path>, label: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&p.export_dir).map_err(|e| e.to_string())?;
    let label = if !label.is_empty() && label.len() <= 20 && label.chars().all(|c| c.is_ascii_digit() || c == '-') {
        label.to_string()
    } else {
        stamp()
    };
    let mut path = p.export_dir.join(format!("mooncli-backup-{label}.zip"));
    let mut i = 2;
    while path.exists() {
        path = p.export_dir.join(format!("mooncli-backup-{label}-{i}.zip"));
        i += 1;
    }
    let file = std::fs::File::create(&path).map_err(|e| format!("没法在 {} 创建文件：{e}", p.export_dir.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let manifest = Manifest {
        app: "mooncli".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        created_at: now_ms(),
        has_history: history_copy.is_some(),
    };
    let mut add = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    add("manifest.json", serde_json::to_string_pretty(&manifest).unwrap().as_bytes())?;
    if let Ok(s) = std::fs::read(&p.settings) {
        add("settings.json", &s)?;
    }
    for name in files::list_backgrounds(&p.backgrounds) {
        if let Ok(b) = std::fs::read(p.backgrounds.join(&name)) {
            add(&format!("backgrounds/{name}"), &b)?;
        }
    }
    if let Some(h) = history_copy {
        add("history.db", &std::fs::read(h).map_err(|e| e.to_string())?)?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// 读出来的备份包内容
pub struct Imported {
    pub settings: Option<serde_json::Value>,
    pub backgrounds: Vec<(String, Vec<u8>)>,
    pub history: Option<Vec<u8>>,
}

pub fn read_package(bytes: &[u8]) -> Result<Imported, String> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "这不是 mooncli 的备份文件（不是 zip）")?;
    let names: Vec<String> =
        zip.file_names().filter_map(|n| n.strip_prefix("backgrounds/")).map(str::to_string).collect();
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let mut f = zip.by_name(name).ok()?;
        if f.size() > MAX_IMPORT_BYTES {
            return None;
        }
        let mut v = Vec::new();
        f.read_to_end(&mut v).ok()?;
        Some(v)
    };
    let manifest: Manifest = read("manifest.json")
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(|m: &Manifest| m.app == "mooncli")
        .ok_or("这不是 mooncli 的备份文件（缺少说明文件）")?;
    let settings = match read("settings.json") {
        Some(b) => {
            let text = String::from_utf8(b).map_err(|_| "备份里的设置文件损坏")?;
            let v: serde_json::Value = mooncli_config::jsonc::parse("settings.json", &text)
                .map_err(|e| format!("备份里的设置文件损坏：{e}"))?;
            Some(v).filter(|v| v.is_object())
        }
        None => None,
    };
    let history = if manifest.has_history { read("history.db") } else { None };
    let mut backgrounds = Vec::new();
    for n in names {
        if let Some(b) = read(&format!("backgrounds/{n}")) {
            backgrounds.push((n, b));
        }
    }
    Ok(Imported { settings, backgrounds, history })
}

/// 在文件管理器里打开文件夹（或选中一个文件）
pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("explorer");
        if path.is_file() {
            c.raw_arg(format!("/select,\"{}\"", path.display()));
        } else {
            c.arg(path);
        }
        c.creation_flags(0x0800_0000).spawn()
    };
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open")
        .arg(if path.is_file() { path.parent().unwrap_or(path) } else { path })
        .spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(root: &Path) -> DataPaths {
        DataPaths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            settings: root.join("config/settings.json"),
            backgrounds: root.join("config/backgrounds"),
            backups: root.join("config/backups"),
            history: root.join("data/history.db"),
            export_dir: root.join("Downloads"),
        }
    }

    #[test]
    fn backups_rotate_and_restore() {
        let d = tempfile::tempdir().unwrap();
        let p = paths(d.path());
        assert_eq!(snapshot_settings(&p, "manual").unwrap(), None);
        files::write_atomic(&p.settings, br#"{"zoom": 1.2}"#).unwrap();
        auto_snapshot(&p);
        auto_snapshot(&p); // 一天内只自动备份一次
        assert_eq!(list_backups(&p).len(), 1);
        for _ in 0..KEEP_BACKUPS + 3 {
            snapshot_settings(&p, "manual").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let list = list_backups(&p);
        assert_eq!(list.len(), KEEP_BACKUPS);
        assert_eq!(read_backup(&p, &list[0].name).unwrap()["zoom"], 1.2);
        assert!(read_backup(&p, "../settings.json").is_err());
    }

    #[test]
    fn export_then_read_package() {
        let d = tempfile::tempdir().unwrap();
        let p = paths(d.path());
        files::write_atomic(&p.settings, br#"{"theme": "frost"}"#).unwrap();
        std::fs::create_dir_all(&p.backgrounds).unwrap();
        std::fs::write(p.backgrounds.join("lake.png"), b"png").unwrap();
        let hist = d.path().join("h.db");
        std::fs::write(&hist, b"sqlite").unwrap();
        let zip = export(&p, Some(&hist), "20261022-1530").unwrap();
        assert!(zip.ends_with("mooncli-backup-20261022-1530.zip"));
        let pkg = read_package(&std::fs::read(zip).unwrap()).unwrap();
        assert_eq!(pkg.settings.unwrap()["theme"], "frost");
        assert_eq!(pkg.backgrounds, vec![("lake.png".to_string(), b"png".to_vec())]);
        assert_eq!(pkg.history.unwrap(), b"sqlite");
        assert!(read_package(b"not a zip").is_err());
    }
}
