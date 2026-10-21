use std::cell::RefCell;
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

pub struct Store {
    pub(crate) conn: Connection,
    filter: Filter,
    /// 懒加载：第一次默认范围搜索时从数据库建好，之后随写入增量更新；删除时作废重建
    pub(crate) index: RefCell<Option<Index>>,
}

const SCHEMA_VERSION: i32 = 1;

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
            ix.finish(id, exit_code);
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
        Ok(self.conn.execute("DELETE FROM commands WHERE text = ?1", [text])?)
    }

    /// 清空某个目录下的全部历史。
    pub fn clear_cwd(&self, cwd: &str) -> Result<usize> {
        self.invalidate();
        Ok(self.conn.execute("DELETE FROM commands WHERE cwd = ?1", [cwd])?)
    }

    pub fn clear_all(&self) -> Result<usize> {
        self.invalidate();
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
        let mut stmt = self.conn.prepare(
            "SELECT id, text, cwd, exit_code, started_at, host FROM commands ORDER BY id",
        )?;
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
            .query_row("SELECT exit_code, duration_ms FROM commands WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
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
}
