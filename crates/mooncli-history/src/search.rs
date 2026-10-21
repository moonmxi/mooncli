//! 历史搜索和排序。排序因素见 docs/04-command-memory.md 第 2 节"排序"。

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rusqlite::types::Value;

use crate::{Result, Store};

const DAY: i64 = 24 * 60 * 60 * 1000;
/// 去重视图最多从数据库取这么多条不同的命令来打分。十万条历史里不同命令通常远少于这个数。
const MAX_CANDIDATES: i64 = 50_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum Scope {
    #[default]
    All,
    Cwd(String),
    Session(String),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub text: String,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub success_only: bool,
    #[serde(default)]
    pub host: Option<String>,
    /// 相同命令只显示一条
    #[serde(default = "yes")]
    pub dedupe: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub now_ms: i64,
    /// 当前目录，用来给"在这里执行过"的命令加分
    #[serde(default)]
    pub cwd: Option<String>,
    /// 当前会话，用来给本会话刚用过的命令加分
    #[serde(default)]
    pub session_id: Option<String>,
}

fn yes() -> bool {
    true
}
fn default_limit() -> usize {
    200
}

impl Query {
    pub fn new(text: impl Into<String>, now_ms: i64) -> Self {
        Self {
            text: text.into(),
            scope: Scope::All,
            success_only: false,
            host: None,
            dedupe: true,
            limit: default_limit(),
            now_ms,
            cwd: None,
            session_id: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    /// 最近一次执行的记录 id
    pub id: i64,
    pub text: String,
    /// 执行次数（不去重时恒为 1）
    pub count: i64,
    pub last_used: i64,
    pub last_cwd: Option<String>,
    pub last_exit: Option<i32>,
    pub host: Option<String>,
    /// 匹配到的字符位置（按字符计），界面用来高亮
    pub matched: Vec<u32>,
    pub score: f64,
}

struct Stats {
    weighted_uses: f64,
    here: i64,
    ok: i64,
    failed: i64,
}

struct Candidate {
    hit: Hit,
    stats: Stats,
}

impl Store {
    pub fn search(&self, q: &Query) -> Result<Vec<Hit>> {
        let mut fz = Fuzzy::new(&q.text);
        let recent = self.recent_in_session(q)?;
        if q.dedupe && q.scope == Scope::All && !q.success_only && q.host.is_none() {
            return self.search_index(q, &mut fz, &recent);
        }
        let mut cands = self.candidates_from_sql(q)?;
        cands.retain_mut(|c| match fz.score(&c.hit.text) {
            Some(m) => {
                c.hit.score = m + rank_bonus(c, q, &recent);
                true
            }
            None => false,
        });
        if q.dedupe {
            cands.sort_unstable_by(|a, b| b.hit.score.total_cmp(&a.hit.score).then(b.hit.id.cmp(&a.hit.id)));
        }
        // 不去重时按时间顺序展示每一次执行（SQL 已经按时间倒序取出）
        cands.truncate(q.limit);
        // 高亮位置比打分慢，只给最终要显示的结果算
        Ok(cands
            .into_iter()
            .map(|mut c| {
                c.hit.matched = fz.indices(&c.hit.text);
                c.hit
            })
            .collect())
    }

    /// 默认范围走内存索引：先只用引用打分，选出前 `limit` 名再生成结果。
    fn search_index(&self, q: &Query, fz: &mut Fuzzy, recent: &[String]) -> Result<Vec<Hit>> {
        self.load_index()?;
        let ix = self.index.borrow();
        let ix = ix.as_ref().expect("刚加载过");
        let mut scored: Vec<(f64, i64, usize)> = Vec::new();
        for (i, e) in ix.entries.iter().enumerate() {
            let Some(m) = fz.score(&e.text) else { continue };
            let stats = Stats {
                weighted_uses: e.uses.iter().map(|&t| decay(q.now_ms - t)).sum(),
                here: q.cwd.as_ref().and_then(|c| e.cwds.get(c)).copied().unwrap_or(0) as i64,
                ok: e.ok as i64,
                failed: e.failed as i64,
            };
            scored.push((m + bonus(&stats, &e.text, q, recent), e.last_id, i));
        }
        let by_rank = |a: &(f64, i64, usize), b: &(f64, i64, usize)| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1));
        if scored.len() > q.limit && q.limit > 0 {
            scored.select_nth_unstable_by(q.limit - 1, by_rank);
            scored.truncate(q.limit);
        }
        scored.sort_unstable_by(by_rank);
        Ok(scored
            .into_iter()
            .take(q.limit)
            .map(|(score, _, i)| {
                let e = &ix.entries[i];
                Hit {
                    id: e.last_id,
                    text: e.text.clone(),
                    count: e.uses.len() as i64,
                    last_used: e.last_used,
                    last_cwd: e.last_cwd.clone(),
                    last_exit: e.last_exit,
                    host: e.host.clone(),
                    matched: fz.indices(&e.text),
                    score,
                }
            })
            .collect())
    }

    fn candidates_from_sql(&self, q: &Query) -> Result<Vec<Candidate>> {
        let mut wheres = vec!["1 = 1".to_string()];
        let mut args: Vec<Value> = Vec::new();
        let arg = |v: Value, args: &mut Vec<Value>| {
            args.push(v);
            format!("?{}", args.len())
        };
        match &q.scope {
            Scope::All => {}
            Scope::Cwd(d) => wheres.push(format!("cwd = {}", arg(Value::Text(d.clone()), &mut args))),
            Scope::Session(s) => wheres.push(format!("session_id = {}", arg(Value::Text(s.clone()), &mut args))),
        }
        if let Some(h) = &q.host {
            wheres.push(format!("host = {}", arg(Value::Text(h.clone()), &mut args)));
        }
        if q.success_only {
            wheres.push("exit_code = 0".into());
        }
        let now = arg(Value::Integer(q.now_ms), &mut args);
        let cwd = arg(q.cwd.clone().map(Value::Text).unwrap_or(Value::Null), &mut args);
        let where_sql = wheres.join(" AND ");
        // 时间衰减：一天内 4、一周内 2、一月内 1、更早 0.5
        let weight = format!(
            "CASE WHEN {now} - started_at < {d} THEN 4.0 WHEN {now} - started_at < {w} THEN 2.0 \
             WHEN {now} - started_at < {m} THEN 1.0 ELSE 0.5 END",
            d = DAY,
            w = 7 * DAY,
            m = 30 * DAY
        );
        // 只用一个 MAX() 聚合，这样 SQLite 的"裸列"会取自 id 最大（最近）的那一行
        let sql = if q.dedupe {
            format!(
                "SELECT MAX(id), text, COUNT(*), started_at, cwd, exit_code, host,
                        SUM({weight}),
                        SUM(CASE WHEN cwd = {cwd} THEN 1 ELSE 0 END),
                        SUM(CASE WHEN exit_code = 0 THEN 1 ELSE 0 END),
                        SUM(CASE WHEN exit_code IS NOT NULL AND exit_code <> 0 THEN 1 ELSE 0 END)
                 FROM commands WHERE {where_sql}
                 GROUP BY text ORDER BY MAX(id) DESC LIMIT {MAX_CANDIDATES}"
            )
        } else {
            format!(
                "SELECT id, text, 1, started_at, cwd, exit_code, host, {weight},
                        CASE WHEN cwd = {cwd} THEN 1 ELSE 0 END,
                        CASE WHEN exit_code = 0 THEN 1 ELSE 0 END,
                        CASE WHEN exit_code IS NOT NULL AND exit_code <> 0 THEN 1 ELSE 0 END
                 FROM commands WHERE {where_sql}
                 ORDER BY started_at DESC, id DESC LIMIT {MAX_CANDIDATES}"
            )
        };
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
            Ok(Candidate {
                hit: Hit {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    count: r.get(2)?,
                    last_used: r.get(3)?,
                    last_cwd: r.get(4)?,
                    last_exit: r.get(5)?,
                    host: r.get(6)?,
                    matched: Vec::new(),
                    score: 0.0,
                },
                stats: Stats { weighted_uses: r.get(7)?, here: r.get(8)?, ok: r.get(9)?, failed: r.get(10)? },
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn recent_in_session(&self, q: &Query) -> Result<Vec<String>> {
        let Some(sid) = &q.session_id else { return Ok(Vec::new()) };
        let mut stmt =
            self.conn.prepare_cached("SELECT text FROM commands WHERE session_id = ?1 ORDER BY id DESC LIMIT 10")?;
        let rows = stmt.query_map([sid], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// 模糊匹配。空查询匹配一切、得 0 分。
struct Fuzzy {
    pattern: Pattern,
    matcher: Matcher,
    buf: Vec<char>,
    prefix: String,
    chars: f64,
    empty: bool,
}

impl Fuzzy {
    fn new(text: &str) -> Self {
        Self {
            pattern: Pattern::parse(text, CaseMatching::Smart, Normalization::Smart),
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
            prefix: text.trim_start().to_string(),
            chars: text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f64,
            empty: text.trim().is_empty(),
        }
    }

    fn score(&mut self, text: &str) -> Option<f64> {
        if self.empty {
            return Some(0.0);
        }
        let raw = self.pattern.score(Utf32Str::new(text, &mut self.buf), &mut self.matcher)?;
        // nucleo 每个匹配字符大约 16 分，换算成 0~1.5 左右再乘 100；前缀匹配额外加 30
        let mut s = (raw as f64 / (16.0 * self.chars)).min(1.5) * 100.0;
        if text.starts_with(&self.prefix) {
            s += 30.0;
        }
        Some(s)
    }

    fn indices(&mut self, text: &str) -> Vec<u32> {
        if self.empty {
            return Vec::new();
        }
        let mut idx = Vec::new();
        self.pattern.indices(Utf32Str::new(text, &mut self.buf), &mut self.matcher, &mut idx);
        idx.sort_unstable();
        idx.dedup();
        idx
    }
}

/// 时间衰减权重，和 SQL 里的 CASE 保持一致：一天内 4、一周内 2、一月内 1、更早 0.5。
fn decay(age_ms: i64) -> f64 {
    if age_ms < DAY {
        4.0
    } else if age_ms < 7 * DAY {
        2.0
    } else if age_ms < 30 * DAY {
        1.0
    } else {
        0.5
    }
}

fn rank_bonus(c: &Candidate, q: &Query, recent: &[String]) -> f64 {
    bonus(&c.stats, &c.hit.text, q, recent)
}

fn bonus(c: &Stats, text: &str, q: &Query, recent: &[String]) -> f64 {
    let mut s = (1.0 + c.weighted_uses).ln() * 12.0;
    if q.cwd.is_some() && c.here > 0 {
        s += 25.0;
    }
    if c.ok > 0 {
        s += 10.0;
    } else if c.failed > 0 {
        s -= 20.0;
    }
    if recent.iter().any(|t| t == text) {
        s += 15.0;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandRecord;

    const NOW: i64 = 1_800_000_000_000;

    fn add(s: &Store, text: &str, cwd: &str, exit: Option<i32>, ago_days: i64, session: &str) {
        let mut r = CommandRecord::new(text, NOW - ago_days * DAY);
        r.cwd = Some(cwd.into());
        r.exit_code = exit;
        r.session_id = Some(session.into());
        r.host = Some("PC".into());
        s.insert(&r).unwrap();
    }

    fn texts(h: &[Hit]) -> Vec<&str> {
        h.iter().map(|h| h.text.as_str()).collect()
    }

    fn sample() -> Store {
        let s = Store::open_in_memory().unwrap();
        for _ in 0..5 {
            add(&s, "git push origin main", "/repo", Some(0), 1, "s1");
        }
        add(&s, "git pull --rebase", "/repo", Some(0), 0, "s1");
        add(&s, "git push --force-with-lease", "/blog", Some(1), 3, "s2");
        add(&s, "npm run build", "/web", Some(0), 40, "s3");
        s
    }

    #[test]
    fn fuzzy_match_with_dedupe_and_counts() {
        let s = sample();
        let mut q = Query::new("git pu", NOW);
        q.cwd = Some("/repo".into());
        let hits = s.search(&q).unwrap();
        assert_eq!(texts(&hits), vec!["git push origin main", "git pull --rebase", "git push --force-with-lease"]);
        assert_eq!(hits[0].count, 5);
        assert!(!hits[0].matched.is_empty());
    }

    #[test]
    fn same_directory_wins_over_raw_frequency() {
        let s = Store::open_in_memory().unwrap();
        for _ in 0..3 {
            add(&s, "cargo test", "/other", Some(0), 10, "x");
        }
        add(&s, "cargo build", "/here", Some(0), 10, "x");
        let mut q = Query::new("cargo", NOW);
        q.cwd = Some("/here".into());
        assert_eq!(s.search(&q).unwrap()[0].text, "cargo build");
    }

    #[test]
    fn only_failed_commands_rank_lower() {
        let s = Store::open_in_memory().unwrap();
        add(&s, "make deploy", "/a", Some(2), 0, "x");
        add(&s, "make dev", "/a", Some(0), 0, "x");
        assert_eq!(texts(&s.search(&Query::new("make", NOW)).unwrap()), vec!["make dev", "make deploy"]);
    }

    #[test]
    fn no_dedupe_lists_every_run_newest_first() {
        let s = sample();
        let mut q = Query::new("push", NOW);
        q.dedupe = false;
        let hits = s.search(&q).unwrap();
        assert_eq!(hits.len(), 6);
        assert!(hits.windows(2).all(|w| w[0].last_used >= w[1].last_used));
        assert!(hits.iter().all(|h| h.count == 1));
    }

    #[test]
    fn scopes_and_success_filter() {
        let s = sample();
        let mut q = Query::new("", NOW);
        q.scope = Scope::Cwd("/blog".into());
        assert_eq!(texts(&s.search(&q).unwrap()), vec!["git push --force-with-lease"]);

        let mut q = Query::new("git", NOW);
        q.success_only = true;
        assert!(!texts(&s.search(&q).unwrap()).contains(&"git push --force-with-lease"));

        let mut q = Query::new("", NOW);
        q.scope = Scope::Session("s3".into());
        assert_eq!(texts(&s.search(&q).unwrap()), vec!["npm run build"]);
    }

    #[test]
    fn empty_query_ranks_by_use() {
        let s = sample();
        let hits = s.search(&Query::new("", NOW)).unwrap();
        assert_eq!(hits[0].text, "git push origin main");
        assert_eq!(hits.len(), 4);
    }

    #[test]
    fn non_matching_query_returns_nothing() {
        assert!(sample().search(&Query::new("zzzq", NOW)).unwrap().is_empty());
    }

    #[test]
    fn index_and_sql_paths_agree() {
        let s = sample();
        let mut a = Query::new("git", NOW);
        a.cwd = Some("/repo".into());
        let mut b = a.clone();
        b.host = Some("PC".into()); // 所有样本都是这台主机，但带了筛选就会走 SQL
        let (ia, ib) = (s.search(&a).unwrap(), s.search(&b).unwrap());
        assert_eq!(texts(&ia), texts(&ib));
        for (x, y) in ia.iter().zip(&ib) {
            assert_eq!((x.id, x.count, x.last_exit), (y.id, y.count, y.last_exit));
            assert!((x.score - y.score).abs() < 1e-9, "{} {} vs {}", x.text, x.score, y.score);
        }
    }

    #[test]
    fn index_follows_writes() {
        let s = sample();
        s.search(&Query::new("", NOW)).unwrap(); // 建好索引
        let id = s.insert(&CommandRecord::new("cargo fmt", NOW)).unwrap().unwrap();
        let hit = |s: &Store| s.search(&Query::new("cargo fmt", NOW)).unwrap().into_iter().next();
        assert_eq!(hit(&s).unwrap().last_exit, None);
        s.finish(id, Some(0), 10).unwrap();
        assert_eq!(hit(&s).unwrap().last_exit, Some(0));
        s.delete_text("cargo fmt").unwrap();
        assert!(hit(&s).is_none());
    }

    #[test]
    fn hundred_thousand_rows_stay_fast() {
        let s = Store::open_in_memory().unwrap();
        let tx = s.conn.unchecked_transaction().unwrap();
        for i in 0..100_000 {
            add(&s, &format!("cmd-{} --flag {}", i % 5000, i % 7), "/d", Some(0), i % 60, "s");
        }
        tx.commit().unwrap();
        let t = std::time::Instant::now();
        s.search(&Query::new("", NOW)).unwrap();
        eprintln!("首次搜索（建索引）耗时 {:?}", t.elapsed());
        let mut worst = std::time::Duration::ZERO;
        // 模拟逐字输入
        for q in ["c", "cm", "cmd", "cmd-", "cmd-4", "cmd-42", "cmd-42 f", "cmd-42 flag"] {
            let t = std::time::Instant::now();
            let hits = s.search(&Query::new(q, NOW)).unwrap();
            worst = worst.max(t.elapsed());
            assert!(!hits.is_empty());
        }
        eprintln!("逐字输入时单次搜索最慢 {worst:?}");
        // 调试构建下放宽；发布构建的目标是 < 16ms（docs/09），CI 用 --release 跑一次
        let limit = if cfg!(debug_assertions) { 500 } else { 16 };
        assert!(worst.as_millis() < limit, "搜索太慢: {worst:?}");
    }
}
