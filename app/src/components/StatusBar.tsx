import type { Breakpoint } from "../layout/layout";
import type { Tab } from "../state/store";

const STATE_LABEL: Record<Tab["shellState"], string> = {
  unknown: "未集成",
  prompt: "等待输入",
  running: "运行中",
  altScreen: "全屏程序",
};

export function StatusBar({ tab, bp, size, backendKind }: { tab?: Tab; bp: Breakpoint; size: string; backendKind: string }) {
  const where = tab?.remote ? tab.remote.host : tab?.cwd;
  return (
    <footer className="status">
      <span title="按键归属：只有“等待输入”时 mooncli 才会处理补全类按键（docs/05 §1）">
        <span className={`sdot s-${tab?.shellState ?? "unknown"}`} />
        {tab?.env?.pwsh ? `pwsh ${tab.env.pwsh}` : "pwsh"} · {STATE_LABEL[tab?.shellState ?? "unknown"]}
      </span>
      {where && <span className="path">{where}</span>}
      {bp !== "sm" && <span>UTF-8</span>}
      {bp !== "sm" && <span>{size}</span>}
      <span className="r">{backendKind === "demo" ? "演示后端" : ""}</span>
    </footer>
  );
}
