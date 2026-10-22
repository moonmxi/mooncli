import { describe, expect, it } from "vitest";
import type { UsageReport } from "../backend/types";
import { accountFor, contextPercent, formatElapsed, formatResetIn, formatTokens, levelOf, pickSession, windowLabel } from "./usage";

describe("windowLabel", () => {
  it("认分钟数不认套餐", () => {
    expect(windowLabel(300)).toBe("5 小时");
    expect(windowLabel(10080)).toBe("本周");
    expect(windowLabel(60)).toBe("1 小时");
    expect(windowLabel(45)).toBe("45 分钟");
    expect(windowLabel(undefined)).toBe("额度");
  });
});

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
  it("各档文案", () => {
    expect(formatResetIn(now + 42 * 60, now)).toBe("42 分钟后重置");
    expect(formatResetIn(now + 2 * 3600 + 18 * 60, now)).toBe("2 小时 18 分后重置");
    expect(formatResetIn(now + 3 * 3600, now)).toBe("3 小时后重置");
    expect(formatResetIn(now + 3 * 86400, now)).toBe("3 天后重置");
    expect(formatResetIn(now - 5, now)).toBe("已重置");
    expect(formatResetIn(undefined, now)).toBeNull();
  });
});

describe("levelOf / contextPercent", () => {
  it("阈值分档", () => {
    expect(levelOf(0.5, 0.7, 0.9)).toBe("ok");
    expect(levelOf(0.7, 0.7, 0.9)).toBe("warn");
    expect(levelOf(0.95, 0.7, 0.9)).toBe("danger");
    expect(contextPercent(250, 200)).toBe(100);
    expect(contextPercent(50, 0)).toBe(0);
  });
});

describe("formatElapsed", () => {
  it("分钟与小时", () => {
    expect(formatElapsed(0, 14 * 60000)).toBe("14m");
    expect(formatElapsed(0, 65 * 60000)).toBe("1h 05m");
  });
});

const report: UsageReport = {
  sessions: [
    { kind: "codex", sessionId: "s-old", cwd: "C:\\work\\a", state: "unknown", updatedAt: 100 },
    { kind: "codex", sessionId: "s-tab", tabSessionId: "tab-1", cwd: "C:\\work\\b", state: "running", updatedAt: 90 },
    { kind: "codex", sessionId: "s-dir", cwd: "c:/work/b/", state: "waiting", updatedAt: 95 },
  ],
  accounts: [{ kind: "codex", updatedAt: 90 }],
};

describe("pickSession", () => {
  it("优先标签页自己的会话", () => {
    const picked = pickSession(report, { id: "tab-1" } as never);
    expect(picked?.session.sessionId).toBe("s-tab");
    expect(picked?.via).toBe("tab");
  });
  it("其次同目录（路径写法不同也能对上）", () => {
    const picked = pickSession(report, { id: "tab-2", cwd: "C:\\work\\B" } as never);
    expect(picked?.session.sessionId).toBe("s-dir");
    expect(picked?.via).toBe("cwd");
  });
  it("最后最近活动的", () => {
    const picked = pickSession(report, { id: "tab-3", cwd: "D:\\elsewhere" } as never);
    expect(picked?.session.sessionId).toBe("s-old");
    expect(picked?.via).toBe("recent");
  });
  it("没有会话时为空", () => {
    expect(pickSession({ sessions: [], accounts: [] })).toBeNull();
  });
  it("accountFor 找到账号用量", () => {
    expect(accountFor(report, "codex")?.kind).toBe("codex");
    expect(accountFor({ sessions: [], accounts: [] }, "codex")).toBeUndefined();
  });
});
