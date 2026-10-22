// 历史面板里的时间、耗时、目录怎么显示。都按中文习惯，尽量短。

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

function startOfDay(t: number): number {
  const d = new Date(t);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const hm = (t: number) => {
  const d = new Date(t);
  return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
};

/** 刚刚 / 5 分钟前 / 3 小时前 / 昨天 14:20 / 10月3日 / 2025年10月3日 */
export function relativeTime(t: number, now = Date.now()): string {
  const ago = now - t;
  if (ago < MIN) return "刚刚";
  if (ago < HOUR) return `${Math.floor(ago / MIN)} 分钟前`;
  const today = startOfDay(now);
  if (t >= today) return `${Math.floor(ago / HOUR)} 小时前`;
  if (t >= today - DAY) return `昨天 ${hm(t)}`;
  const d = new Date(t);
  if (d.getFullYear() === new Date(now).getFullYear()) return `${d.getMonth() + 1}月${d.getDate()}日`;
  return `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`;
}

/** 详情里用的完整时间：今天 14:20 / 昨天 9:05 / 10月3日 14:20 */
export function fullTime(t: number, now = Date.now()): string {
  const today = startOfDay(now);
  if (t >= today) return `今天 ${hm(t)}`;
  if (t >= today - DAY) return `昨天 ${hm(t)}`;
  const d = new Date(t);
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return `${sameYear ? "" : `${d.getFullYear()}年`}${d.getMonth() + 1}月${d.getDate()}日 ${hm(t)}`;
}

/** 分组标题：今天 / 昨天 / 本周 / 更早 */
export function dayGroup(t: number, now = Date.now()): string {
  const today = startOfDay(now);
  if (t >= today) return "今天";
  if (t >= today - DAY) return "昨天";
  if (t >= today - 6 * DAY) return "这一周";
  if (t >= today - 29 * DAY) return "这个月";
  return "更早";
}

/** 320 毫秒 / 4.2 秒 / 3 分 12 秒 / 1 小时 5 分 */
export function duration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "";
  if (ms < 1000) return `${Math.max(0, Math.round(ms))} 毫秒`;
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)} 秒`;
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  if (m < 60) return s % 60 ? `${m} 分 ${s % 60} 秒` : `${m} 分`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h} 小时 ${m % 60} 分` : `${h} 小时`;
}

/** 看起来像用户主目录的前缀：C:\Users\名字、/home/名字、/root */
const HOME_RE = /^(?:[A-Za-z]:\\Users\\[^\\]+|\/home\/[^/]+|\/root|\/Users\/[^/]+)(?=$|[\\/])/;

/** 目录缩短显示：主目录换成 ~，太长时只留最后两级 */
export function shortDir(path: string | null | undefined, home?: string): string {
  if (!path) return "";
  let p = path;
  if (home && p.toLowerCase().startsWith(home.toLowerCase())) p = `~${p.slice(home.length)}`;
  else p = p.replace(HOME_RE, "~");
  const sep = p.includes("\\") ? "\\" : "/";
  const parts = p.split(/[\\/]/).filter(Boolean);
  if (p.length <= 28 || parts.length <= 2) return p;
  return `…${sep}${parts.slice(-2).join(sep)}`;
}

/** 退出码的说明：0 成功、130 被 Ctrl+C 中断，其它是失败 */
export function exitLabel(code: number | null | undefined): { kind: "ok" | "bad" | "stop" | "none"; text: string } {
  if (code === null || code === undefined) return { kind: "none", text: "没有记录结果" };
  if (code === 0) return { kind: "ok", text: "成功" };
  if (code === 130 || code === -1073741510) return { kind: "stop", text: "被中断" };
  return { kind: "bad", text: `失败（代码 ${code}）` };
}
