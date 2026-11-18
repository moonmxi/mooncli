import type { AgentAccountUsage, AgentKind, RateWindow, UsageReport } from "../backend/types";

// Read the rendered viewport so redraws, cursor motion and split UTF-8 packets are already resolved.
export function readRemoteUsage(rows: string[], now: number): AgentAccountUsage | undefined {
  const windows: Partial<Record<"fiveHour" | "sevenDay", RateWindow>> = {};
  let kind: AgentKind | undefined;
  for (let i = 0; i < rows.length; i++) {
    const line = rows[i].replace(/^[\s│┃╭╰─]+|[\s│┃]+$/gu, "");
    const label = /^(current session|current week(?: \(all models\))?|5h limit|weekly limit)\s*(?::|$)/i.exec(line);
    if (!label) continue;
    const agent = /limit$/i.test(label[1]) ? "codex" : "claude";
    let block = line;
    for (const next of rows.slice(i + 1, i + 4)) {
      if (/current (week|session)|(?:weekly|5h) limit/i.test(next)) break;
      block += " " + next;
    }
    const value = /(\d+(?:\.\d+)?)%\s*(used|left|remaining)\b/i.exec(block);
    if (!value || (kind && kind !== agent)) continue;
    kind = agent;
    const pct = Math.min(100, Number(value[1]));
    windows[/week/i.test(label[1]) ? "sevenDay" : "fiveHour"] = { usedPercent: /^used$/i.test(value[2]) ? pct : 100 - pct };
  }
  return kind && { kind, ...windows, source: "terminal", updatedAt: now };
}

export function remoteReport(accounts: AgentAccountUsage[], now: number): UsageReport {
  return { accounts: accounts.filter((a) => now - a.updatedAt < 600), refreshing: [], liveErrors: {
    claude: "在此连接的 Claude Code 中打开 /usage",
    codex: "在此连接的 Codex 中打开 /status",
  } };
}

export function readRemoteFrame(data: string, now: number): AgentAccountUsage | undefined {
  if (data.length > 512) return;
  let v;
  try { v = JSON.parse(data); } catch { return; }
  if (!v || (v.kind !== "claude" && v.kind !== "codex")) return;
  const account: AgentAccountUsage = { kind: v.kind, source: "terminal", updatedAt: now };
  for (const key of ["fiveHour", "sevenDay"] as const) {
    const w = v[key];
    if (typeof w?.usedPercent !== "number" || !Number.isFinite(w.usedPercent)) continue;
    account[key] = { usedPercent: Math.max(0, Math.min(100, w.usedPercent)),
      resetsAt: Number.isSafeInteger(w.resetsAt) ? w.resetsAt : undefined };
  }
  return account.fiveHour || account.sevenDay ? account : undefined;
}
