// 设置 → 关于：版本、链接、检查更新（只在点按钮时联网，问 GitHub 最新的正式版本）、打包日志。
import { useEffect, useState } from "react";
import type { Backend } from "../../backend/types";
import { Group, Row } from "./controls";

const REPO = "moonmxi/mooncli";
const LINKS = [
  { label: "GitHub 主页", url: `https://github.com/${REPO}` },
  { label: "更新日志", url: `https://github.com/${REPO}/releases` },
  { label: "使用文档", url: `https://github.com/${REPO}/blob/main/docs/user/quick-start.md` },
  { label: "反馈问题", url: `https://github.com/${REPO}/issues/new/choose` },
];

/** "v0.2.0" 比 "0.1.1" 新吗（只比数字部分，预发布版本不算） */
export function isNewer(latest: string, current: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/i, "")
      .split(/[.-]/)
      .slice(0, 3)
      .map((x) => Number.parseInt(x, 10) || 0);
  const a = parse(latest);
  const b = parse(current);
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return false;
}

type UpdateState = { kind: "idle" } | { kind: "checking" } | { kind: "latest" } | { kind: "newer"; tag: string; url: string } | { kind: "error"; text: string };

export function AboutSection({ backend }: { backend: Backend }) {
  const [version, setVersion] = useState<string>("");
  const [update, setUpdate] = useState<UpdateState>({ kind: "idle" });
  const [logs, setLogs] = useState<string | null>(null);
  useEffect(() => {
    backend
      .diagnostics()
      .then((d) => setVersion(d.appVersion))
      .catch(() => {});
  }, [backend]);

  const check = async () => {
    setUpdate({ kind: "checking" });
    try {
      const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
      if (!r.ok) throw new Error(`GitHub 返回 ${r.status}`);
      const j = (await r.json()) as { tag_name?: string; html_url?: string };
      if (!j.tag_name) throw new Error("没有找到正式版本");
      setUpdate(isNewer(j.tag_name, version) ? { kind: "newer", tag: j.tag_name, url: j.html_url ?? `https://github.com/${REPO}/releases/latest` } : { kind: "latest" });
    } catch (e) {
      setUpdate({ kind: "error", text: e instanceof Error ? e.message : String(e) });
    }
  };

  const updateText = {
    idle: "只在你点这个按钮时联网，向 GitHub 查询最新的正式版本，不发送任何个人信息。",
    checking: "正在查询…",
    latest: `已经是最新版本（${version}）。`,
    newer: update.kind === "newer" ? `有新版本 ${update.tag}，点「下载」打开发布页。` : "",
    error: update.kind === "error" ? `查询失败：${update.text}` : "",
  }[update.kind];

  return (
    <>
      <div className="about-hero">
        <img src="/mooncli.svg" width={56} height={56} alt="" />
        <div>
          <div className="about-name">
            mooncli <span className="about-ver">{version ? `v${version}` : ""}</span>
          </div>
          <div className="about-tag">为 AI 编程时代设计的 Windows 终端：浅色优先，记得住命令，看得懂 Agent。</div>
        </div>
      </div>
      <Group title="更新">
        <Row label="检查更新" desc={updateText}>
          <span style={{ display: "inline-flex", gap: 8 }}>
            {update.kind === "newer" && (
              <button type="button" className="btn primary" onClick={() => backend.openUrl(update.url)}>
                下载
              </button>
            )}
            <button type="button" className="btn" disabled={update.kind === "checking" || !version} onClick={check}>
              检查更新
            </button>
          </span>
        </Row>
      </Group>
      <Group title="链接">
        <div className="about-links">
          {LINKS.map((l) => (
            <button key={l.url} type="button" className="chip" onClick={() => backend.openUrl(l.url)}>
              {l.label}
            </button>
          ))}
        </div>
      </Group>
      <Group title="反馈问题时">
        <Row label="打包日志" desc={logs ? `已保存到 ${logs}` : "把本机的错误日志和诊断信息（版本、系统、shell 路径，不含命令历史）打成一个 zip，反馈问题时附上。日志只存在你的电脑上，不会自动上传。"}>
          <span style={{ display: "inline-flex", gap: 8 }}>
            {logs && (
              <button type="button" className="btn" onClick={() => backend.dataReveal(logs).catch(() => {})}>
                打开所在文件夹
              </button>
            )}
            <button
              type="button"
              className="btn"
              onClick={() =>
                backend
                  .logsPack()
                  .then(setLogs)
                  .catch((e) => setLogs(`失败：${String(e)}`))
              }
            >
              打包日志
            </button>
          </span>
        </Row>
      </Group>
      <Group title="许可">
        <Row label="mooncli" desc="MIT 许可证开源。附带的第三方组件：Symbols Nerd Font（MIT）、微软 ConPTY（MIT）；界面基于 Tauri、xterm.js、React。">
          <span />
        </Row>
      </Group>
    </>
  );
}
