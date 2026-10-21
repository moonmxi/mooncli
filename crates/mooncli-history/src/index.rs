//! 默认范围（全部历史、去重）搜索用的内存索引。
//!
//! 每次按键都在 SQLite 里对十万行做 GROUP BY 太慢（发布构建约 150ms），
//! 所以按命令文本把统计信息汇总在内存里，写入时增量更新。

use std::collections::HashMap;

use crate::CommandRecord;

#[derive(Debug, Clone)]
pub(crate) struct TextStats {
    pub text: String,
    pub last_id: i64,
    pub last_used: i64,
    pub last_cwd: Option<String>,
    pub last_exit: Option<i32>,
    pub host: Option<String>,
    /// 每次执行的时间，用于按查询时刻计算时间衰减
    pub uses: Vec<i64>,
    pub cwds: HashMap<String, u32>,
    pub ok: u32,
    pub failed: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Index {
    by_text: HashMap<String, usize>,
    pub entries: Vec<TextStats>,
    id_to_text: HashMap<i64, usize>,
}

impl Index {
    pub fn add(&mut self, id: i64, rec: &CommandRecord) {
        let i = match self.by_text.get(&rec.text) {
            Some(&i) => i,
            None => {
                self.entries.push(TextStats {
                    text: rec.text.clone(),
                    last_id: id,
                    last_used: rec.started_at,
                    last_cwd: None,
                    last_exit: None,
                    host: None,
                    uses: Vec::new(),
                    cwds: HashMap::new(),
                    ok: 0,
                    failed: 0,
                });
                self.by_text.insert(rec.text.clone(), self.entries.len() - 1);
                self.entries.len() - 1
            }
        };
        let e = &mut self.entries[i];
        e.uses.push(rec.started_at);
        if let Some(c) = &rec.cwd {
            *e.cwds.entry(c.clone()).or_default() += 1;
        }
        Self::count_exit(e, rec.exit_code);
        if id >= e.last_id {
            e.last_id = id;
            e.last_used = rec.started_at;
            e.last_cwd = rec.cwd.clone();
            e.last_exit = rec.exit_code;
            e.host = rec.host.clone();
        }
        self.id_to_text.insert(id, i);
    }

    /// 命令结束时补退出码。只对还没有退出码的记录调用。
    pub fn finish(&mut self, id: i64, exit: Option<i32>) {
        let Some(&i) = self.id_to_text.get(&id) else { return };
        let e = &mut self.entries[i];
        Self::count_exit(e, exit);
        if e.last_id == id {
            e.last_exit = exit;
        }
    }

    fn count_exit(e: &mut TextStats, exit: Option<i32>) {
        match exit {
            Some(0) => e.ok += 1,
            Some(_) => e.failed += 1,
            None => {}
        }
    }
}
