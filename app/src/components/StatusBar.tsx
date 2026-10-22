import type { Breakpoint } from "../layout/layout";
import type { Tab } from "../state/store";
import { useUi } from "../state/ui";

const STATE_LABEL: Record<Tab["shellState"], string> = {
  unknown: "未集成",
  prompt: "等待输入",
  running: "运行中",
  altScreen: "全屏程序",
};

interface Props {
  tab?: Tab;
  bp: Breakpoint;
  size: string;
  backendKind: string;
  zoom: number;
}

/** 连到远程时状态栏说的是远程那边的情况：正在连接、等待输入、运行中 */
function remoteLabel(tab: Tab): string | null {
  const r = tab.remote;
  if (!r) return null;
  const who = r.user ? `${r.user}@${r.host}` : r.host;
  if (!r.connected) return `正在连接 ${who}`;
  if (tab.shellState === "altScreen") return `${who} · 全屏程序`;
  return `${who} · ${tab.running ? "运行中" : "已连接"}`;
}

export function StatusBar({ tab, bp, size, backendKind, zoom }: Props) {
  const openSettings = useUi((s) => s.openSettings);
  const state = tab?.shellState ?? "unknown";
  const remote = tab ? remoteLabel(tab) : null;
  const where = tab?.remote ? (tab.remote.cwd ?? null) : tab?.cwd;
  return (
    <footer className="status">
      <button
        type="button"
        className="status-btn"
        title={state === "unknown" ? "没有收到 shell 集成的信号，点击查看诊断" : "按键归属：只有“等待输入”时 mooncli 才会处理补全类按键（docs/05 §1）"}
        onClick={() => state === "unknown" && openSettings("shell")}
      >
        <span className={`sdot s-${state}`} />
        {tab?.launch === "windowsPowerShell"
          ? "Windows PowerShell 5.1（临时）"
          : tab?.launch === "installPwsh"
            ? "正在安装 PowerShell 7"
            : tab?.env?.pwsh
              ? `pwsh ${tab.env.pwsh}`
              : "pwsh"}
        {tab?.launch !== "installPwsh" && ` · ${STATE_LABEL[state]}`}
      </button>
      {remote && tab?.remote && (
        <span className={`status-remote${tab.remote.connected ? " on" : ""}`} title={`通过 ${tab.remote.via ?? "ssh"} 连接`}>
          <span className="rdot" />
          {tab.remote.via ?? "ssh"} · {remote}
        </span>
      )}
      {where && <span className="path">{where}</span>}
      {bp !== "sm" && <span>UTF-8</span>}
      {bp !== "sm" && <span>{size}</span>}
      <span className="r">
        {zoom !== 1 && <span title="整体缩放，Ctrl+0 恢复">{Math.round(zoom * 100)}%</span>}
        {backendKind === "demo" && <span>演示后端</span>}
      </span>
    </footer>
  );
}
