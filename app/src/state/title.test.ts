import { describe, expect, it } from "vitest";
import { autoTitle, dirName, displayTitle, guessKind, kindFromChain, metaLine, programName } from "./title";

describe("标题规则", () => {
  const base = { title: {}, cwd: "C:\\Users\\moon\\code\\mooncli", profile: "日常" };

  it("目录名", () => {
    expect(dirName("C:\\Users\\moon\\code\\mooncli\\")).toBe("mooncli");
    expect(dirName("/home/moon/x")).toBe("x");
    expect(dirName("/")).toBe("/");
    expect(dirName("C:\\")).toBe("C:\\");
  });

  it("程序名", () => {
    expect(programName("npm run dev")).toBe("npm");
    expect(programName('& "C:\\Program Files\\x\\claude.exe" --resume')).toBe("claude");
    expect(programName("  ./build.ps1 -Release")).toBe("build");
  });

  it("空闲时显示目录，运行时加上程序名", () => {
    expect(autoTitle(base)).toBe("mooncli");
    expect(autoTitle({ ...base, running: { text: "npm run dev" } })).toBe("npm · mooncli");
    expect(autoTitle({ ...base, remote: { host: "dev-box" }, running: { text: "ssh dev-box" } })).toBe("ssh · dev-box");
  });

  it("优先级：用户 > 程序（仅运行中） > 自动", () => {
    const running = { ...base, running: { text: "claude" } };
    expect(displayTitle({ ...running, title: { program: "重构标签页布局" } })).toBe("重构标签页布局");
    // 程序已退出，程序设的标题失效
    expect(displayTitle({ ...base, title: { program: "重构标签页布局" } })).toBe("mooncli");
    expect(displayTitle({ ...running, title: { program: "x", user: "我的标签" } })).toBe("我的标签");
  });

  it("第二行", () => {
    expect(metaLine({ ...base, git: "main*", running: { text: "claude" } })).toBe("mooncli · main* · claude");
    expect(metaLine({ ...base, remote: { host: "box", user: "root" }, running: { text: "ssh box" } })).toBe(
      "root@box · ssh",
    );
    expect(metaLine(base, true)).toBe("C:\\Users\\moon\\code\\mooncli · pwsh");
  });

  it("程序类型", () => {
    expect(guessKind("claude --continue")).toBe("claude");
    expect(guessKind("codex")).toBe("codex");
    expect(guessKind("ssh root@box")).toBe("ssh");
    expect(guessKind("pnpm i")).toBe("node");
    expect(guessKind(undefined)).toBe("pwsh");
  });
});

describe("kindFromChain", () => {
  it("取进程链里最有辨识度的程序", () => {
    expect(kindFromChain(["cmd.exe", "node.exe", "codex.exe"])).toBe("codex");
    expect(kindFromChain(["cmd.exe", "node.exe"])).toBe("node");
    expect(kindFromChain(["powershell.exe", "ssh.exe"])).toBe("ssh");
    expect(kindFromChain(["mystery.exe"])).toBeUndefined();
    expect(kindFromChain([])).toBeUndefined();
  });
});
