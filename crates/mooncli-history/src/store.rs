use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, params};

use crate::index::Index;
use crate::{Filter, Result, parse_psreadline_history};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// shell 集成实时记录
    Live,
    /// 从 PSReadLine 历史文件导入，没有目录和退出码
    Import,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Live => "live",
            Source::Import => "import",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub text: String,
    pub cwd: Option<String>,
    pub exit_code: Option<i32>,
    /// Unix 毫秒
    pub started_at: i64,
    pub duration_ms: Option<i64>,
    pub session_id: Option<String>,
    pub host: Option<String>,
    pub shell: Option<String>,
    pub source: Source,
}

impl CommandRecord {
    pub fn new(text: impl Into<String>, started_at: i64) -> Self {
        Self {
            text: text.into(),
            cwd: None,
            exit_code: None,
            started_at,
            duration_ms: None,
            session_id: None,
            host: None,
            shell: None,
            source: Source::Live,
        }
    }
}

/// 一次执行
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: i64,
    pub started_at: i64,
    pub cwd: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub host: Option<String>,
    pub shell: Option<String>,
    /// 从 PSReadLine 导入的，没有目录和结果
    pub imported: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub count: i64,
    pub ok: i64,
    pub failed: i64,
    pub first_used: Option<i64>,
    pub avg_duration_ms: Option<i64>,
    pub dirs: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCount {
    pub host: String,
    pub count: i64,
    pub remote: bool,
}

pub struct Store {
    pub(crate) conn: Connection,
    filter: Filter,
    /// 懒加载：第一次默认范围搜索时从数据库建好，之后随写入增量更新；删除时作废重建
    pub(crate) index: RefCell<Option<Index>>,
}

const SCHEMA_VERSION: i32 = 2;

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS commands (
                    id          INTEGER PRIMARY KEY,
                    text        TEXT NOT NULL,
                    cwd         TEXT,
                    exit_code   INTEGER,
                    started_at  INTEGER NOT NULL,
                    duration_ms INTEGER,
                    session_id  TEXT,
                    host        TEXT,
                    shell       TEXT,
                    source      TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_cmd_text ON commands(text);
                CREATE INDEX IF NOT EXISTS idx_cmd_cwd ON commands(cwd, started_at);
                CREATE INDEX IF NOT EXISTS idx_cmd_time ON commands(started_at);",
            )?;
        }
        if version < 2 {
            // 第 2 版：收藏的命令
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS favorites (
                    text     TEXT PRIMARY KEY,
                    added_at INTEGER NOT NULL
                );",
            )?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { conn, filter: Filter::default(), index: RefCell::new(None) })
    }

    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
    }

    /// 写入一条记录。被过滤规则拦下时返回 `None`。
    pub fn insert(&self, rec: &CommandRecord) -> Result<Option<i64>> {
        if self.filter.should_skip(&rec.text) {
            return Ok(None);
        }
        self.conn.execute(
            "INSERT INTO commands (text, cwd, exit_code, started_at, duration_ms, session_id, host, shell, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                rec.text,
                rec.cwd,
                rec.exit_code,
                rec.started_at,
                rec.duration_ms,
                rec.session_id,
                rec.host,
                rec.shell,
                rec.source.as_str()
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        if let Some(ix) = self.index.borrow_mut().as_mut() {
            ix.add(id, rec);
        }
        Ok(Some(id))
    }

    /// 命令结束时补上退出码和耗时。
    pub fn finish(&self, id: i64, exit_code: Option<i32>, duration_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE commands SET exit_code = ?2, duration_ms = ?3 WHERE id = ?1",
            params![id, exit_code, duration_ms],
        )?;
        if let Some(ix) = self.index.borrow_mut().as_mut() {
            ix.finish(id, exit_code, duration_ms);
        }
        Ok(())
    }

    /// 首次启动时导入 PSReadLine 历史。导入的记录按文件顺序排在 `before_ms` 之前，每条间隔 1 毫秒。
    /// 已经导入过（库里有 import 记录）就什么都不做，返回 0。
    pub fn import_psreadline(&self, content: &str, host: Option<&str>, before_ms: i64) -> Result<usize> {
        let already: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM commands WHERE source = 'import'", [], |r| r.get(0))?;
        if already > 0 {
            return Ok(0);
        }
        let cmds = parse_psreadline_history(content);
        let n = cmds.len() as i64;
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0;
        for (i, text) in cmds.into_iter().enumerate() {
            let mut rec = CommandRecord::new(text, before_ms - n + i as i64);
            rec.source = Source::Import;
            rec.host = host.map(str::to_string);
            rec.shell = Some("pwsh".into());
            if self.insert(&rec)?.is_some() {
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn delete_id(&self, id: i64) -> Result<usize> {
        self.invalidate();
        Ok(self.conn.execute("DELETE FROM commands WHERE id = ?1", [id])?)
    }

    /// 删除某条命令的全部执行记录（去重视图里的"删除"）。
    pub fn delete_text(&self, text: &str) -> Result<usize> {
        self.invalidate();
        self.conn.execute("DELETE FROM favorites WHERE text = ?1", [text])?;
        Ok(self.conn.execute("DELETE FROM commands WHERE text = ?1", [text])?)
    }

    /// 收藏或取消收藏一条命令（按文字，不分哪一次执行）。
    pub fn set_favorite(&self, text: &str, on: bool, now_ms: i64) -> Result<()> {
        if on {
            self.conn
                .execute("INSERT OR IGNORE INTO favorites (text, added_at) VALUES (?1, ?2)", params![text, now_ms])?;
        } else {
            self.conn.execute("DELETE FROM favorites WHERE text = ?1", [text])?;
        }
        Ok(())
    }

    pub fn favorites(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare_cached("SELECT text FROM favorites")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 一条命令最近的若干次执行，历史面板的详情里用。
    pub fn runs(&self, text: &str, limit: usize) -> Result<Vec<Run>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, started_at, cwd, exit_code, duration_ms, host, shell, source
             FROM commands WHERE text = ?1 ORDER BY started_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![text, limit as i64], |r| {
            Ok(Run {
                id: r.get(0)?,
                started_at: r.get(1)?,
                cwd: r.get(2)?,
                exit_code: r.get(3)?,
                duration_ms: r.get(4)?,
                host: r.get(5)?,
                shell: r.get(6)?,
                imported: r.get::<_, String>(7)? == "import",
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 一条命令的汇总：总次数、成功/失败次数、第一次用的时间、平均耗时。
    pub fn summary(&self, text: &str) -> Result<Summary> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN exit_code = 0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN exit_code IS NOT NULL AND exit_code <> 0 THEN 1 ELSE 0 END),
                    MIN(started_at),
                    AVG(duration_ms),
                    COUNT(DISTINCT cwd)
             FROM commands WHERE text = ?1",
            [text],
            |r| {
                Ok(Summary {
                    count: r.get(0)?,
                    ok: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    failed: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    first_used: r.get(3)?,
                    avg_duration_ms: r.get::<_, Option<f64>>(4)?.map(|v| v as i64),
                    dirs: r.get(5)?,
                })
            },
        )?)
    }

    /// 历史里出现过的主机，次数多的在前。`shell` 不是 pwsh 的是远程主机（ssh、wsl……）。
    pub fn hosts(&self) -> Result<Vec<HostCount>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT host, COUNT(*), MAX(CASE WHEN shell IS NULL OR shell = 'pwsh' THEN 0 ELSE 1 END)
             FROM commands WHERE host IS NOT NULL GROUP BY host ORDER BY COUNT(*) DESC LIMIT 50",
        )?;
        let rows = stmt
            .query_map([], |r| Ok(HostCount { host: r.get(0)?, count: r.get(1)?, remote: r.get::<_, i64>(2)? == 1 }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 清空某个目录下的全部历史。
    pub fn clear_cwd(&self, cwd: &str) -> Result<usize> {
        self.invalidate();
        Ok(self.conn.execute("DELETE FROM commands WHERE cwd = ?1", [cwd])?)
    }

    /// 删掉 `cutoff_ms` 之前的记录（"只保留最近 N 天"）。收藏不受影响。
    pub fn prune_before(&self, cutoff_ms: i64) -> Result<usize> {
        self.invalidate();
        Ok(self.conn.execute("DELETE FROM commands WHERE started_at < ?1", [cutoff_ms])?)
    }

    /// 把整个库安全地复制一份（正在写入也没关系），导出备份时用。
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let _ = std::fs::remove_file(path);
        self.conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
        Ok(())
    }

    /// 把另一个历史库（导入的备份）合并进来：同一条命令、同一时间的记录只留一份。返回新加的条数。
    pub fn merge_from(&self, path: &Path) -> Result<usize> {
        self.invalidate();
        self.conn.execute("ATTACH DATABASE ?1 AS src", [path.to_string_lossy().as_ref()])?;
        let result = (|| -> Result<usize> {
            let tx = self.conn.unchecked_transaction()?;
            let n = tx.execute(
                "INSERT INTO commands (text, cwd, exit_code, started_at, duration_ms, session_id, host, shell, source)
                 SELECT s.text, s.cwd, s.exit_code, s.started_at, s.duration_ms, s.session_id, s.host, s.shell, s.source
                 FROM src.commands s
                 WHERE NOT EXISTS (SELECT 1 FROM main.commands c WHERE c.text = s.text AND c.started_at = s.started_at)",
                [],
            )?;
            // 老版本的备份里没有收藏表
            let has_favs: bool = tx.query_row(
                "SELECT COUNT(*) > 0 FROM src.sqlite_master WHERE type = 'table' AND name = 'favorites'",
                [],
                |r| r.get(0),
            )?;
            if has_favs {
                tx.execute(
                    "INSERT OR IGNORE INTO main.favorites (text, added_at) SELECT text, added_at FROM src.favorites",
                    [],
                )?;
            }
            tx.commit()?;
            Ok(n)
        })();
        self.conn.execute("DETACH DATABASE src", [])?;
        result
    }

    pub fn clear_all(&self) -> Result<usize> {
        self.invalidate();
        self.conn.execute("DELETE FROM favorites", [])?;
        Ok(self.conn.execute("DELETE FROM commands", [])?)
    }

    fn invalidate(&self) {
        *self.index.borrow_mut() = None;
    }

    pub(crate) fn load_index(&self) -> Result<()> {
        if self.index.borrow().is_some() {
            return Ok(());
        }
        let mut ix = Index::default();
        let mut stmt =
            self.conn.prepare("SELECT id, text, cwd, exit_code, started_at, host FROM commands ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let mut rec = CommandRecord::new(r.get::<_, String>(1)?, r.get(4)?);
            rec.cwd = r.get(2)?;
            rec.exit_code = r.get(3)?;
            rec.host = r.get(5)?;
            ix.add(r.get(0)?, &rec);
        }
        *self.index.borrow_mut() = Some(ix);
        Ok(())
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM commands", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_filters_and_finishes() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert(&CommandRecord::new("git status", 1)).unwrap().unwrap();
        assert_eq!(s.insert(&CommandRecord::new(" secret thing", 2)).unwrap(), None);
        s.finish(id, Some(0), 120).unwrap();
        let (code, dur): (i32, i64) = s
            .conn
            .query_row("SELECT exit_code, duration_ms FROM commands WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((code, dur), (0, 120));
        assert_eq!(s.count().unwrap(), 1);
    }

    #[test]
    fn import_runs_once() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.import_psreadline("ls\ncd ..\n token=abc\n", Some("PC"), 1000).unwrap(), 2);
        assert_eq!(s.import_psreadline("ls\n", Some("PC"), 2000).unwrap(), 0);
        assert_eq!(s.count().unwrap(), 2);
    }

    #[test]
    fn reopen_file_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.db");
        Store::open(&p).unwrap().insert(&CommandRecord::new("echo hi", 1)).unwrap();
        assert_eq!(Store::open(&p).unwrap().count().unwrap(), 1);
    }

    #[test]
    fn deletes() {
        let s = Store::open_in_memory().unwrap();
        for (t, cwd) in [("a", "/x"), ("a", "/y"), ("b", "/x")] {
            let mut r = CommandRecord::new(t, 1);
            r.cwd = Some(cwd.into());
            s.insert(&r).unwrap();
        }
        assert_eq!(s.delete_text("a").unwrap(), 2);
        assert_eq!(s.clear_cwd("/x").unwrap(), 1);
        assert_eq!(s.count().unwrap(), 0);
    }

    #[test]
    fn favorites_runs_and_hosts() {
        let s = Store::open_in_memory().unwrap();
        for (t, host, shell, exit) in
            [("ls", "PC", "pwsh", Some(0)), ("ls", "box", "ssh", Some(2)), ("top", "box", "ssh", None)]
        {
            let mut r = CommandRecord::new(t, 1);
            r.host = Some(host.into());
            r.shell = Some(shell.into());
            r.exit_code = exit;
            r.duration_ms = Some(40);
            s.insert(&r).unwrap();
        }
        s.set_favorite("ls", true, 5).unwrap();
        s.set_favorite("ls", true, 6).unwrap();
        assert_eq!(s.favorites().unwrap().len(), 1);
        let runs = s.runs("ls", 10).unwrap();
        assert_eq!(runs.len(), 2);
        let sum = s.summary("ls").unwrap();
        assert_eq!((sum.count, sum.ok, sum.failed, sum.avg_duration_ms), (2, 1, 1, Some(40)));
        let hosts = s.hosts().unwrap();
        assert_eq!(hosts[0].host, "box");
        assert!(hosts[0].remote);
        assert!(!hosts.iter().find(|h| h.host == "PC").unwrap().remote);
        s.delete_text("ls").unwrap();
        assert!(s.favorites().unwrap().is_empty());
    }

    #[test]
    fn upgrades_version_1_database() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.db");
        {
            let c = Connection::open(&p).unwrap();
            c.execute_batch(
                "CREATE TABLE commands (id INTEGER PRIMARY KEY, text TEXT NOT NULL, cwd TEXT, exit_code INTEGER,
                 started_at INTEGER NOT NULL, duration_ms INTEGER, session_id TEXT, host TEXT, shell TEXT,
                 source TEXT NOT NULL);
                 INSERT INTO commands (text, started_at, source) VALUES ('old', 1, 'live');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let s = Store::open(&p).unwrap();
        s.set_favorite("old", true, 2).unwrap();
        assert_eq!(s.count().unwrap(), 1);
    }

    #[test]
    fn prune_backup_and_merge() {
        let dir = tempfile::tempdir().unwrap();
        let a = Store::open_in_memory().unwrap();
        for (t, at) in [("old", 10), ("new", 1000), ("new", 2000)] {
            a.insert(&CommandRecord::new(t, at)).unwrap();
        }
        a.set_favorite("new", true, 1).unwrap();
        let file = dir.path().join("copy.db");
        a.backup_to(&file).unwrap();
        assert_eq!(a.prune_before(500).unwrap(), 1);
        assert_eq!(a.count().unwrap(), 2);

        let b = Store::open_in_memory().unwrap();
        b.insert(&CommandRecord::new("new", 1000)).unwrap();
        // 已有的那条不重复加
        assert_eq!(b.merge_from(&file).unwrap(), 2);
        assert_eq!(b.count().unwrap(), 3);
        assert!(b.favorites().unwrap().contains("new"));
        assert_eq!(b.merge_from(&file).unwrap(), 0);
    }
}
