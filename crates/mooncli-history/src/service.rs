//! 历史库服务：写入和搜索分开，搜索永远不挡住终端，也不挡住界面（docs/04 第 3 节"性能"）。
//!
//! - **写入**（命令开始、结束，删除、收藏……）走一条连接，加锁只在执行一条 SQL 的那一下。
//! - **搜索**在单独的线程里，用自己的连接和内存索引：
//!   - 索引在启动时分批建（最新的先进来），建的过程中也能搜，结果标为"不完整"；
//!   - 写入后把新记录发给搜索线程增量更新，不用重读数据库；
//!   - 同一路（`lane`，比如历史面板、补全面板）只算最新的查询：排队的旧查询直接作废，
//!     正在算的旧查询每算几千条看一眼，有新查询就放弃；
//!   - 一次查询的完整排序结果缓存起来，翻页时直接切片，高亮位置只给这一页算。
//!
//! 调用方拿到的是一个 `Receiver`，在自己的线程里等结果，界面线程不会被卡住。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};

use crate::index::Index;
use crate::search::{Fuzzy, Scored, hit_from, rank_index};
use crate::{CommandRecord, Filter, Hit, HostCount, Query, Result, Run, Scope, Store, Summary};

/// 建索引时一批读多少行
const LOAD_CHUNK: usize = 5000;

/// 一页结果
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub hits: Vec<Hit>,
    /// 一共有多少条匹配（逐行扫描的查询不知道总数，为 None）
    pub total: Option<usize>,
    pub has_more: bool,
    /// 索引还没建完，结果可能不全（最新的记录总是在里面）
    pub partial: bool,
}

/// 搜索的结果：一页，或者因为有了同一路更新的查询而作废
pub type PageResult = std::result::Result<Page, String>;

/// 作废的查询回复这句话，调用方忽略即可
pub const SUPERSEDED: &str = "superseded";

enum Msg {
    Search(Job),
    Added(i64, CommandRecord),
    Finished(i64, Option<i32>, i64),
    /// 收藏变了：只需要让缓存作废
    Changed,
    /// 删除、清理、合并之后：重建索引
    Reload,
}

struct Job {
    lane: String,
    generation: u64,
    query: Query,
    offset: usize,
    limit: usize,
    reply: Sender<PageResult>,
}

struct Shared {
    writer: Mutex<Store>,
    tx: Mutex<Sender<Msg>>,
    next_generation: AtomicU64,
    /// 每一路最新的查询编号
    latest: Mutex<HashMap<String, u64>>,
}

/// 历史库服务的句柄，可以随便克隆。最后一个句柄释放后，搜索线程自己退出。
#[derive(Clone)]
pub struct HistoryService {
    shared: Arc<Shared>,
}

impl HistoryService {
    /// 打开数据库（不存在就建），起搜索线程。`prepare` 在搜索线程开始建索引之前对写连接执行，
    /// 用来做导入、清理之类的一次性工作。
    pub fn open(path: impl AsRef<Path>, filter: Filter, prepare: impl FnOnce(&Store)) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut writer = Store::open(&path)?;
        writer.set_filter(filter);
        prepare(&writer);
        let reader = Store::open(&path)?;
        Ok(Self::start(writer, reader))
    }

    /// 测试用：两个连接指向同一个文件
    pub fn open_file_for_test(path: &PathBuf) -> Result<Self> {
        Self::open(path, Filter::default(), |_| {})
    }

    fn start(writer: Store, reader: Store) -> Self {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared {
            writer: Mutex::new(writer),
            tx: Mutex::new(tx),
            next_generation: AtomicU64::new(1),
            latest: Mutex::new(HashMap::new()),
        });
        let weak = Arc::downgrade(&shared);
        std::thread::Builder::new()
            .name("mooncli-history".into())
            .spawn(move || Worker::new(reader).run(rx, weak))
            .expect("起搜索线程");
        Self { shared }
    }

    fn send(&self, m: Msg) {
        let _ = self.shared.tx.lock().unwrap().send(m);
    }

    fn write<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        f(&self.shared.writer.lock().unwrap())
    }

    /// 只读地用一下写连接（详情、主机列表、导出备份……这些都很快）
    pub fn read<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        self.write(f)
    }

    /// 改了数据（删除、清理、合并……）：做完让搜索线程重建索引
    pub fn modify<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        let r = self.write(f);
        self.send(Msg::Reload);
        r
    }

    /// 记一条命令。被过滤规则拦下时返回 None。
    pub fn insert(&self, rec: &CommandRecord) -> Result<Option<i64>> {
        let id = self.write(|s| s.insert(rec))?;
        if let Some(id) = id {
            self.send(Msg::Added(id, rec.clone()));
        }
        Ok(id)
    }

    pub fn finish(&self, id: i64, exit_code: Option<i32>, duration_ms: i64) -> Result<()> {
        self.write(|s| s.finish(id, exit_code, duration_ms))?;
        self.send(Msg::Finished(id, exit_code, duration_ms));
        Ok(())
    }

    pub fn set_favorite(&self, text: &str, on: bool, now_ms: i64) -> Result<()> {
        self.write(|s| s.set_favorite(text, on, now_ms))?;
        self.send(Msg::Changed);
        Ok(())
    }

    pub fn delete_text(&self, text: &str) -> Result<usize> {
        self.modify(|s| s.delete_text(text))
    }

    pub fn delete_id(&self, id: i64) -> Result<usize> {
        self.modify(|s| s.delete_id(id))
    }

    pub fn summary(&self, text: &str) -> Result<Summary> {
        self.read(|s| s.summary(text))
    }

    pub fn runs(&self, text: &str, limit: usize) -> Result<Vec<Run>> {
        self.read(|s| s.runs(text, limit))
    }

    pub fn hosts(&self) -> Result<Vec<HostCount>> {
        self.read(|s| s.hosts())
    }

    pub fn count(&self) -> Result<i64> {
        self.read(|s| s.count())
    }

    /// 搜一页。`lane` 是哪一路在搜（同一路只算最新的查询），结果从返回的通道里取。
    pub fn search(&self, lane: &str, query: Query, offset: usize, limit: usize) -> Receiver<PageResult> {
        let (reply, rx) = channel();
        let generation = self.shared.next_generation.fetch_add(1, Ordering::SeqCst);
        self.shared.latest.lock().unwrap().insert(lane.to_string(), generation);
        self.send(Msg::Search(Job { lane: lane.to_string(), generation, query, offset, limit, reply }));
        rx
    }
}

/// 一路的缓存：同样的查询（不算翻页）再来时直接切片
struct Cached {
    key: String,
    version: u64,
    ranked: Vec<Scored>,
    partial: bool,
}

struct Worker {
    store: Store,
    index: Index,
    /// 建索引的下一批从哪个 id 往下读；None 表示建完了
    cursor: Option<i64>,
    /// 数据每变一次加一，缓存跟着作废
    version: u64,
    cache: HashMap<String, Cached>,
}

impl Worker {
    fn new(store: Store) -> Self {
        let mut w = Self { store, index: Index::default(), cursor: None, version: 0, cache: HashMap::new() };
        w.reload();
        w
    }

    fn reload(&mut self) {
        self.index = Index::default();
        self.cache.clear();
        self.version += 1;
        // 从最新的往下读：先同步读一批（几毫秒），之后的查询至少能搜到最近用过的命令；剩下的在空闲时分批补
        self.cursor = self.store.max_id().ok().map(|m| m + 1);
        self.load_chunk();
    }

    fn run(mut self, rx: Receiver<Msg>, shared: std::sync::Weak<Shared>) {
        loop {
            // 索引建完了就安心等消息；没建完就有消息先处理消息，没有就再读一批
            let first = if self.cursor.is_none() {
                match rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => return,
                }
            } else {
                match rx.try_recv() {
                    Ok(m) => Some(m),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                }
            };
            let mut batch: Vec<Msg> = first.into_iter().collect();
            loop {
                match rx.try_recv() {
                    Ok(m) => batch.push(m),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if batch.is_empty() {
                self.load_chunk();
                continue;
            }
            let mut jobs: Vec<Job> = Vec::new();
            for m in batch {
                match m {
                    Msg::Search(j) => {
                        // 同一路只留最新的一个
                        if let Some(old) = jobs.iter().position(|o| o.lane == j.lane) {
                            let _ = jobs.remove(old).reply.send(Err(SUPERSEDED.into()));
                        }
                        jobs.push(j);
                    }
                    Msg::Added(id, rec) => {
                        self.index.add(id, &rec);
                        self.version += 1;
                    }
                    Msg::Finished(id, exit, dur) => {
                        self.index.finish(id, exit, dur);
                        self.version += 1;
                    }
                    Msg::Changed => self.version += 1,
                    Msg::Reload => self.reload(),
                }
            }
            let Some(shared) = shared.upgrade() else { return };
            for job in jobs {
                let latest = || shared.latest.lock().unwrap().get(&job.lane).copied();
                if latest() != Some(job.generation) {
                    let _ = job.reply.send(Err(SUPERSEDED.into()));
                    continue;
                }
                let cancel = || latest() != Some(job.generation);
                let result = self.search(&job, &cancel);
                let _ = job.reply.send(result.unwrap_or_else(|| Err(SUPERSEDED.into())));
            }
        }
    }

    fn load_chunk(&mut self) {
        let Some(below) = self.cursor else { return };
        match self.store.load_index_chunk(&mut self.index, below, LOAD_CHUNK) {
            Ok(next) => self.cursor = next,
            Err(_) => self.cursor = None,
        }
        self.version += 1;
    }

    /// 返回 None 表示中途作废了
    fn search(&mut self, job: &Job, cancel: &dyn Fn() -> bool) -> Option<PageResult> {
        let q = &job.query;
        let mut fz = Fuzzy::new(&q.text);
        let favorites = self.store.favorites().unwrap_or_default();
        let page = if q.dedupe && !matches!(q.scope, Scope::Session(_)) {
            self.search_index(job, &mut fz, &favorites, cancel)?
        } else if !q.dedupe {
            match self.store.scan_runs(q, &mut fz, job.offset, job.limit, cancel) {
                Ok(Some((hits, has_more))) => Page { hits, total: None, has_more, partial: false },
                Ok(None) => return None,
                Err(e) => return Some(Err(e.to_string())),
            }
        } else {
            // 本标签页 + 合并重复：一个标签页的记录不多，直接问数据库
            let mut q2 = q.clone();
            q2.limit = job.offset + job.limit + 1;
            match self.store.search(&q2) {
                Ok(mut all) => {
                    let has_more = all.len() > job.offset + job.limit;
                    let total = if has_more { None } else { Some(all.len()) };
                    let hits: Vec<Hit> = all.drain(job.offset.min(all.len())..).take(job.limit).collect();
                    Page { hits, total, has_more, partial: false }
                }
                Err(e) => return Some(Err(e.to_string())),
            }
        };
        let mut page = page;
        for h in &mut page.hits {
            h.favorite = favorites.contains(&h.text);
        }
        Some(Ok(page))
    }

    fn search_index(
        &mut self,
        job: &Job,
        fz: &mut Fuzzy,
        favorites: &HashSet<String>,
        cancel: &dyn Fn() -> bool,
    ) -> Option<Page> {
        let q = &job.query;
        let key = cache_key(q);
        let partial = self.cursor.is_some();
        let fresh = self.cache.get(&job.lane).is_some_and(|c| c.key == key && c.version == self.version);
        if !fresh {
            let recent = self.store.recent_in_session(q).unwrap_or_default();
            let ranked = rank_index(&self.index, q, fz, &recent, favorites, cancel)?;
            self.cache.insert(job.lane.clone(), Cached { key, version: self.version, ranked, partial });
        }
        let c = &self.cache[&job.lane];
        let end = (job.offset + job.limit).min(c.ranked.len());
        let start = job.offset.min(end);
        let hits = c.ranked[start..end].iter().map(|s| hit_from(&self.index, s, q, fz)).collect();
        Some(Page { hits, total: Some(c.ranked.len()), has_more: end < c.ranked.len(), partial: c.partial })
    }
}

/// 决定排序结果的字段（翻页、当前时刻不算：时间衰减按天分档，几分钟内的差别可以忽略）
fn cache_key(q: &Query) -> String {
    format!(
        "{}\u{1}{:?}\u{1}{}{}{}\u{1}{:?}\u{1}{:?}\u{1}{:?}\u{1}{:?}",
        q.text, q.scope, q.success_only, q.failed_only, q.favorites_only, q.host, q.sort, q.cwd, q.session_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sort;

    const NOW: i64 = 1_800_000_000_000;

    fn service() -> (tempfile::TempDir, HistoryService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = HistoryService::open_file_for_test(&dir.path().join("h.db")).unwrap();
        (dir, svc)
    }

    fn rec(text: &str, at: i64) -> CommandRecord {
        let mut r = CommandRecord::new(text, at);
        r.cwd = Some("C:/w".into());
        r
    }

    fn page(svc: &HistoryService, lane: &str, q: Query, offset: usize, limit: usize) -> Page {
        svc.search(lane, q, offset, limit).recv().unwrap().unwrap()
    }

    #[test]
    fn pages_through_ranked_results_and_sees_new_writes() {
        let (_d, svc) = service();
        for i in 0..250 {
            svc.insert(&rec(&format!("echo {i}"), NOW - 250_000 + i * 1000)).unwrap();
        }
        let mut q = Query::new("", NOW);
        q.sort = Sort::Recent;
        let p1 = page(&svc, "panel", q.clone(), 0, 100);
        assert_eq!((p1.hits.len(), p1.total, p1.has_more), (100, Some(250), true));
        assert_eq!(p1.hits[0].text, "echo 249");
        let p3 = page(&svc, "panel", q.clone(), 200, 100);
        assert_eq!((p3.hits.len(), p3.has_more), (50, false));
        assert_eq!(p3.hits.last().unwrap().text, "echo 0");
        // 新写入的马上能搜到，收藏标记也在
        let id = svc.insert(&rec("cargo build", NOW)).unwrap().unwrap();
        svc.finish(id, Some(0), 100).unwrap();
        svc.set_favorite("cargo build", true, NOW).unwrap();
        let p = page(&svc, "panel", Query::new("carg", NOW), 0, 10);
        assert_eq!(p.hits[0].text, "cargo build");
        assert!(p.hits[0].favorite);
        assert_eq!(p.hits[0].last_exit, Some(0));
        assert!(!p.hits[0].matched.is_empty());
        // 删除后重建索引
        svc.delete_text("cargo build").unwrap();
        assert!(page(&svc, "panel", Query::new("carg", NOW), 0, 10).hits.is_empty());
    }

    #[test]
    fn non_dedupe_scans_page_by_page() {
        let (_d, svc) = service();
        for i in 0..30 {
            svc.insert(&rec(if i % 2 == 0 { "git status" } else { "ls" }, NOW + i)).unwrap();
        }
        let mut q = Query::new("git", NOW);
        q.dedupe = false;
        let p1 = page(&svc, "panel", q.clone(), 0, 10);
        assert_eq!((p1.hits.len(), p1.has_more, p1.total), (10, true, None));
        let p2 = page(&svc, "panel", q, 10, 10);
        assert_eq!((p2.hits.len(), p2.has_more), (5, false));
        assert!(p2.hits.iter().all(|h| h.text == "git status"));
    }

    #[test]
    fn newer_query_in_same_lane_supersedes_older() {
        let (_d, svc) = service();
        for i in 0..20_000 {
            svc.insert(&rec(&format!("command number {i}"), NOW + i)).unwrap();
        }
        // 连发三个：前两个要么作废、要么已经算完；最后一个一定有结果
        let a = svc.search("panel", Query::new("c", NOW), 0, 10);
        let b = svc.search("panel", Query::new("co", NOW), 0, 10);
        let other = svc.search("complete", Query::new("comm", NOW), 0, 5);
        let c = svc.search("panel", Query::new("com 19999", NOW), 0, 10);
        for r in [a.recv().unwrap(), b.recv().unwrap()] {
            if let Err(e) = r {
                assert_eq!(e, SUPERSEDED);
            }
        }
        assert_eq!(c.recv().unwrap().unwrap().hits[0].text, "command number 19999");
        // 别的路不受影响
        assert_eq!(other.recv().unwrap().unwrap().hits.len(), 5);
    }

    /// 十万条历史：第一页、逐字输入、翻页都要快；搜索进行时写入不被挡住。
    /// CI 用 `cargo test --release -p mooncli-history hundred_thousand` 跑发布构建的版本。
    #[test]
    fn hundred_thousand_service_stays_responsive() {
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.db");
        {
            let s = Store::open(&path).unwrap();
            let tx = s.conn.unchecked_transaction().unwrap();
            for i in 0..100_000i64 {
                let mut r = rec(&format!("cmd-{} --flag {}", i % 5000, i % 7), NOW - (i % 60) * 86_400_000);
                r.exit_code = Some(0);
                r.host = Some("PC".into());
                s.insert(&r).unwrap();
            }
            tx.commit().unwrap();
        }
        let svc = HistoryService::open_file_for_test(&path).unwrap();
        // 索引还在建的时候也能马上拿到第一页
        let t = Instant::now();
        let first = page(&svc, "panel", Query::new("cmd-4", NOW), 0, 100);
        eprintln!("建索引期间第一页：{:?}（partial = {}）", t.elapsed(), first.partial);
        assert!(!first.hits.is_empty());
        // 等索引建完
        let t = Instant::now();
        while page(&svc, "panel", Query::new("", NOW), 0, 1).partial {
            assert!(t.elapsed() < Duration::from_secs(60), "索引建太久");
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("建完索引：{:?}", t.elapsed());
        let mut worst = Duration::ZERO;
        for q in ["c", "cm", "cmd", "cmd-", "cmd-4", "cmd-42", "cmd-42 f", "cmd-42 flag"] {
            let t = Instant::now();
            let p = page(&svc, "panel", Query::new(q, NOW), 0, 100);
            worst = worst.max(t.elapsed());
            assert!(!p.hits.is_empty());
        }
        eprintln!("逐字输入单次最慢：{worst:?}");
        // 翻页走缓存
        let mut q = Query::new("", NOW);
        q.sort = Sort::Recent;
        page(&svc, "panel", q.clone(), 0, 100);
        let t = Instant::now();
        let p = page(&svc, "panel", q.clone(), 2000, 100);
        let paging = t.elapsed();
        eprintln!("翻到第 21 页：{paging:?}");
        assert_eq!(p.hits.len(), 100);
        // 一边搜一边写：写入只等写锁，不等搜索
        let rx = svc.search("panel", Query::new("f", NOW), 0, 100);
        let t = Instant::now();
        svc.insert(&rec("echo during search", NOW)).unwrap();
        let write = t.elapsed();
        eprintln!("搜索进行中写入一条：{write:?}");
        let _ = rx.recv().unwrap();
        // 调试构建下放宽；发布构建的目标是一次按键 < 16ms（docs/09）
        let (search_limit, page_limit, write_limit) =
            if cfg!(debug_assertions) { (800, 100, 200) } else { (16, 5, 20) };
        assert!(worst.as_millis() < search_limit, "搜索太慢: {worst:?}");
        assert!(paging.as_millis() < page_limit, "翻页太慢: {paging:?}");
        assert!(write.as_millis() < write_limit, "写入被挡住: {write:?}");
    }

    #[test]
    fn finish_is_counted_once() {
        let mut ix = Index::default();
        ix.add(1, &rec("x", 1));
        ix.finish(1, Some(1), 5);
        ix.finish(1, Some(1), 5);
        assert_eq!(ix.entries[0].failed, 1);
        // 加进来时已经有退出码的，结束消息不再计
        let mut done = rec("y", 2);
        done.exit_code = Some(0);
        ix.add(2, &done);
        ix.finish(2, Some(0), 5);
        assert_eq!(ix.entries[1].ok, 1);
    }
}
