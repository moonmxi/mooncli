import { describe, expect, it } from "vitest";
import { rebuild, snapshot } from "./restore";
import type { Tab } from "./store";

const tab = (id: string, extra: Partial<Tab> = {}): Tab => ({ id, profile: "日常", title: {}, shellState: "prompt", ...extra });

describe("恢复上次的会话", () => {
  it("保存再恢复：目录、名字、置顶、颜色、分屏布局都在，会话 id 是新的", () => {
    const tabs: Tab[] = [
      tab("a", { cwd: "C:/work", title: { user: "主力" }, pinned: true, color: "blue" }),
      tab("b", {
        cwd: "D:/x",
        split: { tree: { dir: "row", children: [{ pane: "b" }, { dir: "column", children: [{ pane: "p1" }, { pane: "p2" }] }] }, focus: "p2" },
      }),
      tab("p1", { cwd: "D:/x/web", paneOf: "b" }),
      tab("p2", { cwd: "D:/x/api", paneOf: "b" }),
    ];
    const saved = snapshot(tabs, "b");
    expect(saved.active).toBe(1);
    let n = 0;
    const { tabs: back, activeId } = rebuild(JSON.parse(JSON.stringify(saved)), () => `n${++n}`);
    expect(back.map((t) => [t.id, t.initialCwd, t.paneOf])).toEqual([
      ["n1", "C:/work", undefined],
      ["n2", "D:/x", undefined],
      ["n3", "D:/x/web", "n2"],
      ["n4", "D:/x/api", "n2"],
    ]);
    expect(back[0]).toMatchObject({ title: { user: "主力" }, pinned: true, color: "blue" });
    expect(back[1].split).toEqual({ tree: { dir: "row", children: [{ pane: "n2" }, { dir: "column", children: [{ pane: "n3" }, { pane: "n4" }] }] }, focus: "n4" });
    expect(activeId).toBe("n2");
  });

  it("坏掉的数据不出错", () => {
    expect(rebuild({ version: 1, tabs: [{ panes: [] }], active: 3 })).toEqual({ tabs: [], activeId: undefined });
  });
});
