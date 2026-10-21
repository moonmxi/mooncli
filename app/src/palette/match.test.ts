import { describe, expect, it } from "vitest";
import { bestMatch, fuzzyMatch } from "./match";

describe("命令面板匹配", () => {
  it("连续子串优先，并给出高亮位置", () => {
    const m = fuzzyMatch("新建标签页", "标签")!;
    expect(m.indices).toEqual([2, 3]);
    expect(m.score).toBeGreaterThan(fuzzyMatch("标题和签名", "标签")!.score);
  });

  it("英文按顺序的字母也算匹配，词首加分", () => {
    expect(fuzzyMatch("Zoom In", "zi")).not.toBeNull();
    expect(fuzzyMatch("Zoom In", "iz")).toBeNull();
    expect(fuzzyMatch("git push origin", "gpo")!.score).toBeGreaterThan(fuzzyMatch("igpxo", "gpo")!.score);
  });

  it("空查询匹配一切", () => {
    expect(fuzzyMatch("任何", "  ")).toEqual({ score: 0, indices: [] });
  });

  it("可以用英文别名找中文命令", () => {
    expect(bestMatch("放大", ["zoom in"], "zoom")).not.toBeNull();
    expect(bestMatch("放大", ["zoom in"], "xyz")).toBeNull();
  });
});
