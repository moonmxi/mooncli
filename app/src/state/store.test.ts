import { beforeEach, describe, expect, it } from "vitest";
import { focusedTab, listView, orderedTabs, panesOf, useStore } from "./store";

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

describe("分屏", () => {
  it("分屏的窗格不进标签列表，焦点跟着新窗格走", () => {
    s().addTab("a");
    expect(s().splitPane("row", "p1")).toBe("p1");
    expect(orderedTabs(s().tabs).map((t) => t.id)).toEqual(["a"]);
    expect(focusedTab(s())?.id).toBe("p1");
    s().splitPane("column", "p2");
    expect(panesOf(s().tabs[0])).toEqual(["a", "p1", "p2"]);
    s().focusNextPane();
    expect(focusedTab(s())?.id).toBe("a");
  });

  it("列表里显示获得焦点的窗格的内容，身份是标签页自己的", () => {
    s().addTab("a");
    s().rename("a", "我的标签");
    s().splitPane("row", "p1");
    s().apply("p1", { type: "cwd", host: null, path: "D:/work" });
    const v = listView(s().tabs, s().tabs[0]);
    expect(v).toMatchObject({ id: "a", cwd: "D:/work" });
    expect(v.title.user).toBe("我的标签");
  });

  it("关掉标签页自己那个窗格：下一个窗格接替它的位置和名字", () => {
    s().addTab("x");
    s().addTab("a");
    s().togglePin("a");
    s().rename("a", "接替测试");
    s().splitPane("row", "p1");
    s().closePane("a");
    const ids = s().tabs.map((t) => t.id);
    expect(ids).toEqual(["x", "p1"]);
    const heir = s().tabs.find((t) => t.id === "p1")!;
    expect(heir.paneOf).toBeUndefined();
    expect(heir.pinned).toBe(true);
    expect(heir.title.user).toBe("接替测试");
    expect(heir.split).toBeUndefined();
    expect(s().activeId).toBe("p1");
  });

  it("关闭标签页时连同窗格一起关", () => {
    s().addTab("a");
    s().splitPane("row", "p1");
    s().closeTab("a");
    expect(s().tabs).toEqual([]);
  });
});

describe("拖动排序", () => {
  it("往下拖放在目标后面，往上拖放在目标前面，不跨置顶分界", () => {
    for (const id of ["a", "b", "c"]) s().addTab(id);
    s().moveTab("a", "c");
    expect(s().tabs.map((t) => t.id)).toEqual(["b", "c", "a"]);
    s().moveTab("a", "b");
    expect(s().tabs.map((t) => t.id)).toEqual(["a", "b", "c"]);
    s().togglePin("a");
    s().moveTab("c", "a");
    expect(s().tabs.map((t) => t.id)).toEqual(["a", "b", "c"]);
  });
});
