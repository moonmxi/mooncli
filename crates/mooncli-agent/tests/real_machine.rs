//! 真机检查：在装了 Claude Code / Codex 的电脑上手动跑，确认数据层拿到的是真实数据。
//! CI 上没有这些程序，所以默认忽略：`cargo test -p mooncli-agent --test real_machine -- --ignored --nocapture`
//! 用 MOONCLI_PROBE_CWD 指定一个跑过 Agent 的目录，可以看按目录找当前会话的结果；
//! MOONCLI_PROBE_AGENT_DIR 换一个 spool 目录（实测时 bridge 写到别处）。

use std::time::{Duration, Instant};

use mooncli_agent::{AgentKind, CollectOptions, Focus, codex_live};

fn agent_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("MOONCLI_PROBE_AGENT_DIR") {
        return d.into();
    }
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("dev.moonmxi.mooncli")
        .join("agent")
}

#[test]
#[ignore = "需要本机装好并登录 Codex"]
fn codex_live_quota() {
    let exe = codex_live::find_codex_exe(None).expect("PATH 里应能找到 codex");
    println!("codex: {}", exe.display());
    let t = Instant::now();
    let account = codex_live::fetch(&exe, Duration::from_secs(30)).expect("应能查到额度");
    println!("用时 {:?}：{account:#?}", t.elapsed());
    assert!(account.five_hour.is_some() || account.seven_day.is_some());
}

#[test]
#[ignore = "读本机真实的会话文件与 spool"]
fn collect_real_files() {
    let opts = CollectOptions::defaults(agent_dir());
    let t = Instant::now();
    let report = mooncli_agent::collect(&opts, &[], None);
    println!("额度（用时 {:?}）：", t.elapsed());
    for a in &report.accounts {
        println!("  {a:?}");
    }
    if let Ok(cwd) = std::env::var("MOONCLI_PROBE_CWD") {
        for kind in [AgentKind::Claude, AgentKind::Codex] {
            let focus = Focus { kind, tab_id: "probe".into(), cwd: Some(cwd.clone()), since: 0 };
            let t = Instant::now();
            let r = mooncli_agent::collect(&opts, &[], Some(&focus));
            println!("{kind:?} 当前会话（用时 {:?}）：{:?}", t.elapsed(), r.current);
        }
    }
}
