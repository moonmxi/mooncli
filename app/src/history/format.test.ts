import { describe, expect, it } from "vitest";
import { dayGroup, duration, exitLabel, relativeTime, shortDir } from "./format";

const NOW = new Date(2026, 9, 22, 15, 30).getTime();

describe("历史面板的显示格式", () => {
  it("相对时间", () => {
    expect(relativeTime(NOW - 20_000, NOW)).toBe("刚刚");
    expect(relativeTime(NOW - 5 * 60_000, NOW)).toBe("5 分钟前");
    expect(relativeTime(NOW - 3 * 3_600_000, NOW)).toBe("3 小时前");
    expect(relativeTime(new Date(2026, 9, 21, 9, 5).getTime(), NOW)).toBe("昨天 9:05");
    expect(relativeTime(new Date(2026, 2, 3).getTime(), NOW)).toBe("3月3日");
    expect(relativeTime(new Date(2024, 2, 3).getTime(), NOW)).toBe("2024年3月3日");
  });

  it("按天分组", () => {
    expect(dayGroup(NOW - 1000, NOW)).toBe("今天");
    expect(dayGroup(new Date(2026, 9, 21, 23).getTime(), NOW)).toBe("昨天");
    expect(dayGroup(new Date(2026, 9, 18).getTime(), NOW)).toBe("这一周");
    expect(dayGroup(new Date(2026, 9, 1).getTime(), NOW)).toBe("这个月");
    expect(dayGroup(new Date(2025, 9, 1).getTime(), NOW)).toBe("更早");
  });

  it("耗时", () => {
    expect(duration(320)).toBe("320 毫秒");
    expect(duration(4200)).toBe("4.2 秒");
    expect(duration(192_000)).toBe("3 分 12 秒");
    expect(duration(3_900_000)).toBe("1 小时 5 分");
    expect(duration(null)).toBe("");
  });

  it("目录和退出码", () => {
    expect(shortDir("C:\\Users\\moon\\code\\mooncli", "C:\\Users\\moon")).toBe("~\\code\\mooncli");
    expect(shortDir("/very/long/path/to/some/project/dir")).toBe("…/project/dir");
    expect(shortDir("C:\\Users\\moon\\code\\blog")).toBe("~\\code\\blog");
    expect(shortDir("/home/dev")).toBe("~");
    expect(shortDir("/root/x")).toBe("~/x");
    expect(shortDir("/rooted/x")).toBe("/rooted/x");
    expect(exitLabel(0).kind).toBe("ok");
    expect(exitLabel(130).kind).toBe("stop");
    expect(exitLabel(2).text).toBe("失败（代码 2）");
    expect(exitLabel(null).kind).toBe("none");
  });
});
