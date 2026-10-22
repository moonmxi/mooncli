import { describe, expect, it } from "vitest";
import type { AgentAccountUsage } from "../backend/types";
import type { Tab } from "../state/store";
import { claudeSnippet, codexSnippet } from "./manual";
import { agentFocus, compactSummary, formatResetIn, formatTokens, levelOf, shownPercent, worstPercent } from "./usage";

describe("formatTokens", () => {
  it("k / M 缩写", () => {
    expect(formatTokens(950)).toBe("950");
    expect(formatTokens(126_000)).toBe("126k");
    expect(formatTokens(1_500)).toBe("1.5k");
    expect(formatTokens(1_200_000)).toBe("1.2M");
  });
});

describe("formatResetIn", () => {
  const now = 1_792_627_200;
  it("各档文案，过了重置点为空", () => {
    expect(formatResetIn(now + 42 * 60, now)).toBe("42 分钟");
    expect(formatResetIn(now + 2 * 3600 + 18 * 60, now)).toBe("2 小时 18 分");
    expect(formatResetIn(now + 3 * 3600, now)).toBe("3 小时");
    expect(formatResetIn(now + 3 * 86400, now)).toBe("3 天");
    expect(formatResetIn(now - 5, now)).toBeNull();
    expect(formatResetIn(undefined, now)).toBeNull();
  });
});

describe("额度显示", () => {
  const now = 1000;
  const claude: AgentAccountUsage = {
    kind: "claude",
    fiveHour: { usedPercent: 45.6, windowMinutes: 300, resetsAt: now + 60 },
    sevenDay: { usedPercent: 6, windowMinutes: 10080, resetsAt: now + 86400 },
    updatedAt: now,
    source: "statusLine",
  };
  it("阈值分档", () => {
    expect(levelOf(0.5, 0.7, 0.9)).toBe("ok");
    expect(levelOf(0.7, 0.7, 0.9)).toBe("warn");
    expect(levelOf(0.95, 0.7, 0.9)).toBe("danger");
  });
  it("过了重置点按 0% 显示", () => {
    expect(shownPercent(claude.fiveHour!, now)).toBe(45.6);
    expect(shownPercent(claude.fiveHour!, now + 61)).toBe(0);
  });
  it("状态栏摘要：5 小时 · 7 天，缺的窗口写 –", () => {
    expect(compactSummary(claude, now)).toBe("Claude 46% · 6%");
    expect(compactSummary({ ...claude, kind: "codex", fiveHour: undefined }, now)).toBe("Codex – · 6%");
    expect(compactSummary(undefined, now)).toBeNull();
    expect(worstPercent([claude], now)).toBe(45.6);
  });
});

describe("agentFocus", () => {
  const tab = (running?: Tab["running"], extra: Partial<Tab> = {}): Tab => ({
    id: "t1",
    profile: "日常",
    title: {},
    cwd: "D:/work",
    shellState: "running",
    running,
    ...extra,
  });
  it("只有本机正在跑 Claude Code / Codex 时才查上下文", () => {
    expect(agentFocus(tab({ kind: "claude", since: 5 }))).toEqual({ kind: "claude", tabId: "t1", cwd: "D:/work", sinceMs: 5 });
    expect(agentFocus(tab({ kind: "codex", since: 7 }))?.kind).toBe("codex");
    expect(agentFocus(tab({ kind: "node", since: 5 }))).toBeUndefined();
    expect(agentFocus(tab(undefined))).toBeUndefined();
    expect(agentFocus(tab({ kind: "claude", since: 5, remote: true }))).toBeUndefined();
    expect(agentFocus(undefined)).toBeUndefined();
  });
});

describe("手动配置", () => {
  it("路径换成正斜杠，hooks 三个事件", () => {
    const c = JSON.parse(claudeSnippet("C:\\Program Files\\mooncli\\mooncli-bridge.exe"));
    expect(c.statusLine.command).toBe('"C:/Program Files/mooncli/mooncli-bridge.exe" claude-statusline');
    expect(Object.keys(c.hooks)).toEqual(["UserPromptSubmit", "Notification", "Stop"]);
    expect(c.hooks.Stop[0].hooks[0].command).toBe('"C:/Program Files/mooncli/mooncli-bridge.exe" claude-hook');
    expect(codexSnippet("C:\\a b\\mooncli-bridge.exe")).toBe("notify = ['C:/a b/mooncli-bridge.exe', 'codex-notify']");
  });
});
