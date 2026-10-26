// 历史面板的虚拟列表：几千上万行时只渲染看得见的几十行。行高固定（命令一行、分组标题一行），
// 位置直接算出来，不用量 DOM。

export const ROW_HEIGHT = 32;
export const GROUP_HEIGHT = 28;

export type VItem = { kind: "group"; label: string; top: number } | { kind: "row"; index: number; top: number };

/** 把结果排成带分组标题的列表：groupOf 返回每行所属的分组，和上一行不同时插一个标题 */
export function layoutItems(count: number, groupOf: (i: number) => string): { items: VItem[]; rowTop: number[]; height: number } {
  const items: VItem[] = [];
  const rowTop: number[] = new Array(count);
  let top = 0;
  let last = "";
  for (let i = 0; i < count; i++) {
    const g = groupOf(i);
    if (g && g !== last) {
      items.push({ kind: "group", label: g, top });
      top += GROUP_HEIGHT;
    }
    last = g;
    items.push({ kind: "row", index: i, top });
    rowTop[i] = top;
    top += ROW_HEIGHT;
  }
  return { items, rowTop, height: top };
}

/** 看得见的那一段 [start, end)，上下各多留 overscan 像素，滚动时不露白 */
export function visibleRange(items: VItem[], scrollTop: number, viewport: number, overscan = 300): [number, number] {
  const from = Math.max(0, scrollTop - overscan);
  const to = scrollTop + viewport + overscan;
  // 二分找第一个底边在 from 之下的项
  let lo = 0;
  let hi = items.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    const it = items[mid];
    const bottom = it.top + (it.kind === "row" ? ROW_HEIGHT : GROUP_HEIGHT);
    if (bottom <= from) lo = mid + 1;
    else hi = mid;
  }
  let end = lo;
  while (end < items.length && items[end].top < to) end++;
  return [lo, end];
}

/** 让第 top 像素开始、高 h 的一行完整露出来：返回新的 scrollTop（不用动时原样返回） */
export function scrollToShow(scrollTop: number, viewport: number, top: number, h = ROW_HEIGHT, margin = GROUP_HEIGHT): number {
  if (top - margin < scrollTop) return Math.max(0, top - margin);
  if (top + h > scrollTop + viewport) return top + h - viewport;
  return scrollTop;
}
