import { describe, expect, it } from "vitest";
import type { Diagnostics, SessionStats } from "../../backend/types";
import { diagnose } from "./SettingsPage";

const diag = (over: Partial<Diagnostics> = {}): Diagnostics => ({
  appVersion: "0.1.0",
  os: "windows",
  osVersion: "10.0.19045.5011",
  shellPath: "C:/pwsh.exe",
  shellKind: "Pwsh",
  integrationEnabled: true,
  bundledConpty: false,
  settingsPath: "",
  historyPath: "",
  historyCount: 0,
  backgroundsDir: "",
  ...over,
});
const stats = (over: Partial<SessionStats> = {}): SessionStats => ({
  outputBytes: 4096,
  trustedMarks: 0,
  untrustedMarks: 0,
  privateMessages: 0,
  ageMs: 10_000,
  ...over,
});

describe("未集成的原因", () => {
  it("已集成时不提示", () => expect(diagnose(true, stats(), diag())).toBe(""));
  it("集成被关掉", () => expect(diagnose(false, stats(), diag({ integrationEnabled: false }))).toContain("关掉"));
  it("系统 ConPTY 吞掉了标记", () => expect(diagnose(false, stats(), diag())).toContain("系统自带的 ConPTY"));
  it("附带 ConPTY 也没收到：查脚本", () => expect(diagnose(false, stats(), diag({ bundledConpty: true }))).toContain("黄色"));
  it("只有别的 shell 发的标记", () => expect(diagnose(false, stats({ untrustedMarks: 3 }), diag())).toContain("不是本机 pwsh"));
  it("还没有输出", () => expect(diagnose(false, stats({ outputBytes: 0 }), diag())).toContain("稍等"));
});
