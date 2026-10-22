//! 真机检查：在装了 Claude Code / Codex 的电脑上手动跑，确认数据层拿到的是真实数据。
//! CI 上没有这些程序，所以默认忽略：`cargo test -p mooncli-agent --test real_machine -- --ignored --nocapture`

use std::time::{Duration, Instant};

use mooncli_agent::{CollectOptions, codex_live};

#[test]
#[ignore = "需要本机装好并登录 Codex"]
fn codex_live_quota() {
    let exe = codex_live::find_codex_exe(None).expect("PATH 里应能找到 codex");
    println!("codex: {}", exe.display());
    let t = Instant::now();
    let account = codex_live::fetch(&exe, Duration::from_secs(30)).expect("应能查到额度");
    println!("用时 {:?}：{account:#?}", t.elapsed());
    assert!(account.primary.is_some() || account.secondary.is_some());
}

#[test]
#[ignore = "读本机真实的会话文件与 spool"]
fn collect_real_files() {
    let agent_dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("dev.moonmxi.mooncli")
        .join("agent");
    let report = mooncli_agent::collect_all(&CollectOptions::defaults(agent_dir), &[]);
    for s in report.sessions.iter().take(6) {
        println!(
            "{:?} {} model={:?} ctx={:?} state={:?} source={:?} cwd={:?}",
            s.kind, s.session_id, s.model, s.context, s.state, s.source, s.cwd
        );
    }
    for a in &report.accounts {
        println!("account {a:?}");
    }
}
