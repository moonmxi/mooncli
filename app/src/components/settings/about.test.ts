import { describe, expect, it } from "vitest";
import { isNewer } from "./AboutSection";

describe("isNewer", () => {
  it("按数字比较版本号", () => {
    expect(isNewer("v0.2.0", "0.1.1")).toBe(true);
    expect(isNewer("v0.1.10", "0.1.9")).toBe(true);
    expect(isNewer("v0.1.1", "0.1.1")).toBe(false);
    expect(isNewer("v0.1.0", "0.1.1")).toBe(false);
    expect(isNewer("1.0.0", "0.9.9")).toBe(true);
  });
});
