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

export function StatusBar({ tab, bp, size, backendKind, zoom }: Props) {
  const openSettings = useUi((s) => s.openSettings);
  const state = tab?.shellState ?? "unknown";
  const where = tab?.remote ? tab.remote.host : tab?.cwd;
  return (
    <footer className="status">
      <button
        type="button"
        className="status-btn"
        title={state === "unknown" ? "没有收到 shell 集成的信号，点击查看诊断" : "按键归属：只有“等待输入”时 mooncli 才会处理补全类按键（docs/05 §1）"}
        onClick={() => state === "unknown" && openSettings("shell")}
      >
        <span className={`sdot s-${state}`} />
        {tab?.env?.pwsh ? `pwsh ${tab.env.pwsh}` : "pwsh"} · {STATE_LABEL[state]}
      </button>
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
