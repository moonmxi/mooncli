import { useMemo } from "react";
import { useSettings } from "../state/settings";

const labels: Record<string, string> = {
  "America/Chicago": "芝加哥", "Asia/Shanghai": "上海", "America/New_York": "纽约",
  "America/Los_Angeles": "洛杉矶", "Europe/London": "伦敦", "Asia/Tokyo": "东京", UTC: "UTC",
};
const intl = Intl as typeof Intl & { supportedValuesOf?(key: "timeZone"): string[] };
const zones = [...new Set([...Object.keys(labels), ...(intl.supportedValuesOf?.("timeZone") ?? [])])];

export function ZoneClock({ now }: { now: number }) {
  const saved = useSettings((s) => s.settings.timeZone);
  const zone = zones.includes(saved) ? saved : "America/Chicago";
  const formatter = useMemo(() => new Intl.DateTimeFormat("zh-CN", {
    timeZone: zone, hour: "2-digit", minute: "2-digit", hourCycle: "h23",
  }), [zone]);
  return <label className="zone-clock" title={zone}>
    <span>{labels[zone] ?? zone.split("/").pop()?.replaceAll("_", " ")} {formatter.format(now)}</span>
    <select aria-label="显示时区" value={zone} onChange={(e) => useSettings.getState().update((s) => { s.timeZone = e.target.value; })}>
      {zones.map((z) => <option key={z} value={z}>{labels[z] ?? z}</option>)}
    </select>
  </label>;
}
