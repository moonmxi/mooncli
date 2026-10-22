//! 时间小工具：工作区里没有引 chrono，这里只做两件小事——
//! 取文件 mtime 的 epoch 秒、把会话文件里的 RFC3339 时间戳转成 epoch 秒。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn file_mtime_epoch(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// 解析 RFC3339 / ISO 8601 时间戳（`2026-10-22T10:15:30.123Z`、`+08:00` 偏移）为 epoch 秒。
/// 只精确到秒；格式不对返回 None，不猜。
pub fn parse_rfc3339_epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let num = |from: usize, len: usize| -> Option<i64> {
        s.get(from..from + len)?.parse::<i64>().ok()
    };
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, min, sec) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    // 时区：Z 或 ±hh:mm / ±hhmm / ±hh
    let rest = &s[19..];
    let offset_secs: i64 = if rest.is_empty() || rest.starts_with('Z') || rest.starts_with('z') {
        0
    } else if rest.starts_with('+') || rest.starts_with('-') {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        let digits: String = rest[1..].chars().filter(|c| *c != ':').collect();
        if digits.len() < 2 || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let oh: i64 = digits[..2].parse().ok()?;
        let om: i64 = if digits.len() >= 4 { digits[2..4].parse().ok()? } else { 0 };
        sign * (oh * 3600 + om * 60)
    } else if rest.starts_with('.') {
        // 小数秒后跟 Z（Claude 转录就是这个格式）：跳过小数部分再看时区
        let after = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        if after.is_empty() || after.starts_with('Z') || after.starts_with('z') {
            0
        } else {
            return parse_rfc3339_epoch(&format!("{}T{}{}", &s[..10], &s[11..19], after));
        }
    } else {
        return None;
    };
    // civil 日期 → 天数（Howard Hinnant 的 days_from_civil）
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + min * 60 + sec - offset_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zulu_with_fraction() {
        assert_eq!(parse_rfc3339_epoch("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_rfc3339_epoch("2026-10-22T10:15:30.123Z"), Some(1792664130));
    }

    #[test]
    fn parses_offset() {
        assert_eq!(parse_rfc3339_epoch("1970-01-01T08:00:01+08:00"), Some(1));
        assert_eq!(parse_rfc3339_epoch("1970-01-01T00:00:00-05:00"), Some(18000));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_rfc3339_epoch("not a time"), None);
        assert_eq!(parse_rfc3339_epoch("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_rfc3339_epoch(""), None);
    }
}
