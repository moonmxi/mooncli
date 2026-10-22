import type { Tab } from "../state/store";

function shortDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  return `${Math.floor(m / 60)}h${m % 60 ? `${m % 60}m` : ""}`;
}

/** 标签页右侧的状态：运行中的呼吸点、成功 ✓、失败 ✗ 退出码、需要注意的光环点（docs/08 §8）。 */
export function TabStatus({ tab, now, showDuration }: { tab: Tab; now: number; showDuration: boolean }) {
  if (tab.attention) return <span className="dot attn" title="需要你看一眼" />;
  if (tab.exited) return <span className="bad-mark">已退出</span>;
  if (tab.running) {
    const ms = now - tab.running.since;
    return (
      <span className="stat">
        {showDuration && ms > 10_000 && <span className="dur">{shortDuration(ms)}</span>}
        <span className="dot run" />
      </span>
    );
  }
  const last = tab.lastCommand;
  // 刚结束的命令：5 秒内显示结果（后台标签页一直保留到切过去，由 attention 负责）
  if (last && now - last.endedAt < 5000 && last.exitCode !== null) {
    return last.exitCode === 0 ? (
      <span className="ok-mark">✓</span>
    ) : (
      <span className="bad-mark">✗ {last.exitCode}</span>
    );
  }
  if (tab.remote?.connected) return <span className="dot remote" title={`已连接 ${tab.remote.host}`} />;
  return null;
}
