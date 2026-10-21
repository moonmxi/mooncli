import { beforeEach, describe, expect, it } from "vitest";
import { orderedTabs, useStore } from "./store";

const s = () => useStore.getState();

beforeEach(() => {
  useStore.setState({ tabs: [], activeId: undefined, focused: true });
});

describe("标签页状态", () => {
  it("新建、切换、关闭", () => {
    s().addTab("a");
    s().addTab("b");
    expect(s().activeId).toBe("b");
    s().cycle(1);
    expect(s().activeId).toBe("a");
    s().closeTab("a");
    expect(s().activeId).toBe("b");
  });

  it("命令开始和结束", () => {
    s().addTab("a");
    s().apply("a", { type: "commandStarted", text: "claude" }, 1000);
    expect(s().tabs[0].running).toMatchObject({ text: "claude", kind: "claude" });
    s().apply("a", { type: "title", text: "重构标签页布局", activity: "✳", informative: true });
    expect(s().tabs[0].title.program).toBe("重构标签页布局");
    s().apply("a", { type: "commandFinished", text: "claude", exitCode: 0 }, 5000);
    const t = s().tabs[0];
    expect(t.running).toBeUndefined();
    expect(t.title.program).toBeUndefined();
    expect(t.lastCommand).toMatchObject({ exitCode: 0, durationMs: 4000 });
  });

  it("没信息量的标题清掉程序标题", () => {
    s().addTab("a");
    s().apply("a", { type: "title", text: "x", activity: null, informative: true });
    s().apply("a", { type: "title", text: "pwsh", activity: null, informative: false });
    expect(s().tabs[0].title.program).toBeUndefined();
  });

  it("后台标签页的长命令结束和响铃会提醒，切过去后清掉", () => {
    s().addTab("a");
    s().addTab("b");
    s().apply("a", { type: "commandStarted", text: "cargo build" }, 0);
    s().apply("a", { type: "commandFinished", text: "cargo build", exitCode: 0 }, 60_000);
    expect(s().tabs[0].attention?.reason).toBe("long-command");
    s().activate("a");
    expect(s().tabs[0].attention).toBeUndefined();
    s().apply("b", { type: "bell" });
    expect(s().tabs[1].attention?.reason).toBe("bell");
  });

  it("当前标签页响铃不提醒", () => {
    s().addTab("a");
    s().apply("a", { type: "bell" });
    expect(s().tabs[0].attention).toBeUndefined();
  });

  it("ssh 后远程目录不覆盖本机目录", () => {
    s().addTab("a");
    s().apply("a", { type: "env", host: "MOON-PC" });
    s().apply("a", { type: "cwd", host: "MOON-PC", path: "C:/code" });
    s().apply("a", { type: "commandStarted", text: "ssh box" });
    s().apply("a", { type: "cwd", host: "box", path: "/root" });
    expect(s().tabs[0]).toMatchObject({ cwd: "C:/code", remote: { host: "box" } });
    s().apply("a", { type: "commandFinished", text: "ssh box", exitCode: 0 });
    expect(s().tabs[0].remote).toBeUndefined();
  });

  it("记住上次展开宽度，收起状态不覆盖它", () => {
    s().rememberOpenWidth(280);
    s().rememberOpenWidth(52);
    expect(s().lastOpenWidth).toBe(280);
  });

  it("置顶的标签排在前面，切换顺序跟着显示顺序走", () => {
    s().addTab("a");
    s().addTab("b");
    s().addTab("c");
    s().togglePin("c");
    expect(orderedTabs(s().tabs).map((t) => t.id)).toEqual(["c", "a", "b"]);
    s().activateIndex(0);
    expect(s().activeId).toBe("c");
    s().cycle(1);
    expect(s().activeId).toBe("a");
    s().closeTab("a");
    expect(s().activeId).toBe("b");
  });

  it("复制标签页带上起始目录", () => {
    s().addTab("a", { cwd: "C:/code" });
    expect(s().tabs[0]).toMatchObject({ initialCwd: "C:/code", cwd: "C:/code" });
  });
});
