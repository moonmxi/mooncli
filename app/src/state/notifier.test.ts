import { describe, expect, it } from "vitest";
import { agentTransitions, attentionMessage } from "./notifier";

describe("提醒", () => {
  it("Agent 状态变成等你确认 / 跑完一轮时提醒关联的标签页", () => {
    const before = new Map([
      ["s1", "running" as const],
      ["s2", "running" as const],
      ["s3", "running" as const],
    ]);
    const out = agentTransitions(before, {
      accounts: [],
      sessions: [
        { kind: "claude", sessionId: "s1", tabSessionId: "t1", state: "waiting", updatedAt: 1, source: "statusLine" },
        { kind: "codex", sessionId: "s2", tabSessionId: "t2", state: "done", updatedAt: 1, source: "sessionFile" },
        { kind: "codex", sessionId: "s3", state: "done", updatedAt: 1, source: "sessionFile" },
        { kind: "codex", sessionId: "new", tabSessionId: "t4", state: "done", updatedAt: 1, source: "sessionFile" },
      ],
    });
    expect(out).toEqual([
      { tabId: "t1", reason: "agent-waiting", text: "Claude Code 在等你确认" },
      { tabId: "t2", reason: "agent-done", text: "Codex 本轮已完成" },
    ]);
  });
  it("长命令的通知写明成败", () => {
    const msg = attentionMessage({
      id: "a",
      profile: "日常",
      title: {},
      cwd: "D:/work",
      shellState: "prompt",
      attention: { reason: "long-command", at: 1 },
      lastCommand: { text: "cargo build", exitCode: 101, endedAt: 1 },
    });
    expect(msg).toEqual({ title: "mooncli · work", body: "失败：cargo build" });
  });
});
