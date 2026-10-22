import { describe, expect, it } from "vitest";
import { alertAction, attentionMessage } from "./notifier";

describe("提醒", () => {
  it("Agent 提醒：正看着不打扰，在别的标签页窗口内提示，窗口在后台弹系统通知", () => {
    expect(alertAction(true, true, true)).toBe("none");
    expect(alertAction(true, true, false)).toBe("toast");
    expect(alertAction(false, true, true)).toBe("inApp");
    expect(alertAction(false, false, true)).toBe("toast");
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
