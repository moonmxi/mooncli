import { describe, expect, it } from "vitest";
import { GROUP_HEIGHT, layoutItems, ROW_HEIGHT, scrollToShow, visibleRange } from "./virtual";

describe("虚拟列表", () => {
  it("分组变化时插标题，位置连续", () => {
    const groups = ["今天", "今天", "昨天", "", ""];
    const { items, rowTop, height } = layoutItems(5, (i) => groups[i]);
    expect(items.map((x) => (x.kind === "group" ? x.label : x.index))).toEqual(["今天", 0, 1, "昨天", 2, 3, 4]);
    expect(rowTop).toEqual([GROUP_HEIGHT, GROUP_HEIGHT + ROW_HEIGHT, 2 * GROUP_HEIGHT + 2 * ROW_HEIGHT, 2 * GROUP_HEIGHT + 3 * ROW_HEIGHT, 2 * GROUP_HEIGHT + 4 * ROW_HEIGHT]);
    expect(height).toBe(2 * GROUP_HEIGHT + 5 * ROW_HEIGHT);
  });

  it("一万行只取看得见的一段", () => {
    const { items } = layoutItems(10_000, () => "");
    const [s, e] = visibleRange(items, 5000 * ROW_HEIGHT, 600, 0);
    expect(s).toBe(5000);
    expect(e - s).toBeLessThanOrEqual(Math.ceil(600 / ROW_HEIGHT) + 1);
    const [s2, e2] = visibleRange(items, 0, 600, 300);
    expect(s2).toBe(0);
    expect(e2).toBe(Math.ceil(900 / ROW_HEIGHT));
  });

  it("选中的行滚进视野", () => {
    expect(scrollToShow(0, 320, 100)).toBe(0);
    expect(scrollToShow(0, 320, 400)).toBe(400 + ROW_HEIGHT - 320);
    expect(scrollToShow(1000, 320, 500)).toBe(500 - GROUP_HEIGHT);
  });
});
