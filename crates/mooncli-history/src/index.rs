//! 去重搜索用的内存索引（全部历史、当前目录、某台主机、成败、收藏都走这里，只有"本标签页"和不去重走数据库）。
//!
//! 每次按键都在 SQLite 里对十万行做 GROUP BY 太慢（发布构建约 150ms），
//! 所以按命令文本把统计信息汇总在内存里，写入时增量更新。

use std::collections::{HashMap, HashSet};

use crate::CommandRecord;

#[derive(Debug, Clone)]
pub(crate) struct TextStats {
    pub text: String,
    pub last_id: i64,
    pub last_used: i64,
    pub last_cwd: Option<String>,
    pub last_exit: Option<i32>,
    pub last_duration: Option<i64>,
    pub host: Option<String>,
    /// 每次执行的时间，用于按查询时刻计算时间衰减
    pub uses: Vec<i64>,
    pub cwds: HashMap<String, u32>,
    /// 每台主机上的次数和最近一次的情况：按主机筛选时显示的是那台主机上的结果
    pub hosts: Vec<HostStats>,
    pub ok: u32,
    pub failed: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct HostStats {
    pub host: String,
    pub count: u32,
    pub last_id: i64,
    pub last_used: i64,
    pub last_cwd: Option<String>,
    pub last_exit: Option<i32>,
    pub last_duration: Option<i64>,
}

#[derive(Debug, Default)]
pub(crate) struct Index {
    by_text: HashMap<String, usize>,
    pub entries: Vec<TextStats>,
    id_to_text: HashMap<i64, usize>,
    /// 加进来时还没有退出码的记录：只有它们收到"结束"时才计成败，同一条结束消息来两次也不会重复算
    open: HashSet<i64>,
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
                    last_duration: None,
                    host: None,
                    uses: Vec::new(),
                    cwds: HashMap::new(),
                    hosts: Vec::new(),
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
        if rec.exit_code.is_none() {
            self.open.insert(id);
        }
        if let Some(h) = &rec.host {
            let pos = match e.hosts.iter().position(|x| &x.host == h) {
                Some(p) => p,
                None => {
                    e.hosts.push(HostStats {
                        host: h.clone(),
                        count: 0,
                        last_id: id,
                        last_used: rec.started_at,
                        last_cwd: None,
                        last_exit: None,
                        last_duration: None,
                    });
                    e.hosts.len() - 1
                }
            };
            let hs = &mut e.hosts[pos];
            hs.count += 1;
            if (rec.started_at, id) >= (hs.last_used, hs.last_id) {
                hs.last_id = id;
                hs.last_used = rec.started_at;
                hs.last_cwd = rec.cwd.clone();
                hs.last_exit = rec.exit_code;
                hs.last_duration = rec.duration_ms;
            }
        }
        // "最近一次"按执行时间算：导入的旧记录 id 更大，但时间更早
        if (rec.started_at, id) >= (e.last_used, e.last_id) {
            e.last_id = id;
            e.last_used = rec.started_at;
            e.last_cwd = rec.cwd.clone();
            e.last_exit = rec.exit_code;
            e.last_duration = rec.duration_ms;
            e.host = rec.host.clone();
        }
        self.id_to_text.insert(id, i);
    }

    /// 命令结束时补退出码。已经知道退出码的记录忽略。
    pub fn finish(&mut self, id: i64, exit: Option<i32>, duration_ms: i64) {
        let Some(&i) = self.id_to_text.get(&id) else { return };
        if !self.open.remove(&id) {
            return;
        }
        let e = &mut self.entries[i];
        Self::count_exit(e, exit);
        if e.last_id == id {
            e.last_exit = exit;
            e.last_duration = Some(duration_ms);
        }
        if let Some(hs) = e.hosts.iter_mut().find(|h| h.last_id == id) {
            hs.last_exit = exit;
            hs.last_duration = Some(duration_ms);
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
