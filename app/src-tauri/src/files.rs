//! 设置文件和背景图的读写。

use std::path::{Path, PathBuf};

use base64::Engine;

/// 读设置文件。文件不存在、写坏了都返回空对象（界面会用默认值），写坏的文件另存一份备份。
pub fn read_settings(path: &Path) -> serde_json::Value {
    let Ok(text) = std::fs::read_to_string(path) else { return serde_json::json!({}) };
    match mooncli_config::jsonc::parse::<serde_json::Value>(&path.display().to_string(), &text) {
        Ok(v) if v.is_object() => v,
        Ok(_) | Err(_) => {
            let backup = path.with_extension("json.broken");
            let _ = std::fs::copy(path, &backup);
            eprintln!("mooncli: 设置文件格式不对，已备份到 {}，这次使用默认设置", backup.display());
            serde_json::json!({})
        }
    }
}

/// 先写临时文件再改名，写到一半断电也不会留下半个文件。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
const MAX_BACKGROUND_BYTES: usize = 30 * 1024 * 1024;

/// 只保留文件名里安全的字符，防止 `../` 之类跑出背景图目录。
fn safe_name(name: &str) -> Option<String> {
    let base = Path::new(name).file_name()?.to_string_lossy().into_owned();
    let cleaned: String =
        base.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    let ext = Path::new(&cleaned).extension()?.to_string_lossy().to_ascii_lowercase();
    (IMAGE_EXTS.contains(&ext.as_str()) && !cleaned.starts_with('.')).then_some(cleaned)
}

pub fn save_background(dir: &Path, file_name: &str, data_base64: &str) -> Result<String, String> {
    let name = safe_name(file_name).ok_or("只支持 png、jpg、webp、gif、bmp 图片")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data_base64).map_err(|_| "图片数据损坏")?;
    if bytes.len() > MAX_BACKGROUND_BYTES {
        return Err("图片太大了（超过 30MB）".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // 重名时加序号
    let mut target: PathBuf = dir.join(&name);
    let stem = Path::new(&name).file_stem().unwrap().to_string_lossy().into_owned();
    let ext = Path::new(&name).extension().unwrap().to_string_lossy().into_owned();
    let mut i = 2;
    while target.exists() {
        target = dir.join(format!("{stem}-{i}.{ext}"));
        i += 1;
    }
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target.file_name().unwrap().to_string_lossy().into_owned())
}

pub fn load_background(dir: &Path, name: &str) -> Result<String, String> {
    let name = safe_name(name).ok_or("背景图名称不对")?;
    let bytes = std::fs::read(dir.join(&name)).map_err(|_| format!("找不到背景图 {name}"))?;
    let ext = Path::new(&name).extension().unwrap().to_string_lossy().to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

pub fn list_backgrounds(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| safe_name(&e.file_name().to_string_lossy()))
        .collect();
    out.sort();
    out
}

pub fn delete_background(dir: &Path, name: &str) -> Result<(), String> {
    let name = safe_name(name).ok_or("背景图名称不对")?;
    std::fs::remove_file(dir.join(name)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitized() {
        assert_eq!(safe_name("../../etc/passwd"), None);
        // 不管哪个平台，结果里都不能再有路径分隔符
        assert!(safe_name("..\\..\\x.png").is_none_or(|n| !n.contains(['\\', '/'])));
        assert_eq!(safe_name("晨雾 远山.JPG").as_deref(), Some("晨雾 远山.JPG"));
        assert_eq!(safe_name("a;b.png").as_deref(), Some("a_b.png"));
        assert_eq!(safe_name(".hidden.png"), None);
        assert_eq!(safe_name("x.exe"), None);
    }

    #[test]
    fn save_load_list_delete() {
        let dir = std::env::temp_dir().join(format!("mooncli-bg-{}", std::process::id()));
        let png = base64::engine::general_purpose::STANDARD.encode([0x89, b'P', b'N', b'G']);
        let a = save_background(&dir, "moon.png", &png).unwrap();
        let b = save_background(&dir, "moon.png", &png).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("moon.png", "moon-2.png"));
        assert_eq!(list_backgrounds(&dir), vec!["moon-2.png", "moon.png"]);
        assert!(load_background(&dir, "moon.png").unwrap().starts_with("data:image/png;base64,"));
        delete_background(&dir, "moon.png").unwrap();
        assert_eq!(list_backgrounds(&dir), vec!["moon-2.png"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_settings_fall_back_to_empty() {
        let dir = std::env::temp_dir().join(format!("mooncli-set-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, "{ // 注释\n \"zoom\": 1.1, }").unwrap();
        assert_eq!(read_settings(&p)["zoom"], 1.1);
        std::fs::write(&p, "{ broken").unwrap();
        assert_eq!(read_settings(&p), serde_json::json!({}));
        assert!(dir.join("settings.json.broken").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
