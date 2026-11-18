import { describe, expect, it } from "vitest";
import { readRemoteFrame, readRemoteUsage, remoteReport } from "./remote";
import { mergeSettings } from "../state/settings";
import { shownPercent } from "./usage";

describe("Agent quota", () => {
  it("starts disabled, with only the status line selected", () => {
    const a = mergeSettings({}).agent;
    expect([a.enabled, a.widget.style, a.widget.statusBar]).toEqual([false, "off", true]);
    expect(shownPercent({ usedPercent: 110 }, 1)).toBe(100);
  });
  it("reads Claude and Codex screens and distinguishes remaining from used", () => {
    expect(readRemoteUsage(["Current session", "██ 23% used", "Current week (all models)", "77% used"], 1))
      .toMatchObject({ kind: "claude", fiveHour: { usedPercent: 23 }, sevenDay: { usedPercent: 77 } });
    expect(readRemoteUsage(["│ 5h limit: ███ 75% left", "│ Weekly limit: 90% left"], 1))
      .toMatchObject({ kind: "codex", fiveHour: { usedPercent: 25 }, sevenDay: { usedPercent: 10 } });
    expect(readRemoteUsage(["Current session", "Current week (all models): 10% used"], 1)?.fiveHour).toBeUndefined();
    expect(readRemoteUsage(["Disk usage: 75% used"], 1)).toBeUndefined();
  });
  it("limits telemetry fields and expires disconnected samples", () => {
    const a = readRemoteFrame('{"kind":"claude","fiveHour":{"usedPercent":110},"sevenDay":{"usedPercent":-3}}', 100)!;
    expect([a.fiveHour?.usedPercent, a.sevenDay?.usedPercent]).toEqual([100, 0]);
    expect(remoteReport([a], 699).accounts).toHaveLength(1);
    expect(remoteReport([a], 700).accounts).toHaveLength(0);
    for (const s of ["", "null", "[]", '{"kind":"claude","fiveHour":{"usedPercent":"25"}}']) {
      expect(readRemoteFrame(s, 1)).toBeUndefined();
    }
  });
});
