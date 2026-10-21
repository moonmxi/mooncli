// 命令面板的模糊匹配：按顺序出现的字符都算匹配，连续、在词首、在开头的加分。中文按字匹配。

export interface Match {
  score: number;
  /** 匹配到的字符位置，用来高亮 */
  indices: number[];
}

const isBoundary = (prev: string | undefined, c: string) =>
  prev === undefined || /[\s\-_/·:：.>（(]/.test(prev) || (/[a-z]/.test(prev) && /[A-Z]/.test(c));

export function fuzzyMatch(text: string, query: string): Match | null {
  const q = query.trim().toLowerCase();
  if (!q) return { score: 0, indices: [] };
  const chars = [...text];
  const lower = chars.map((c) => c.toLowerCase());
  const qs = [...q].filter((c) => c !== " ");
  // 先试连续子串，分数最高
  const joined = lower.join("");
  const sub = joined.indexOf(qs.join(""));
  if (sub >= 0) {
    // 子串位置按字符算（中文是多字节，但这里 chars 已经按码点拆好了）
    const start = [...joined.slice(0, sub)].length;
    const indices = qs.map((_, i) => start + i);
    return { score: 100 + qs.length * 10 - start + (isBoundary(chars[start - 1], chars[start]) ? 20 : 0), indices };
  }
  const indices: number[] = [];
  let score = 0;
  let last = -2;
  let j = 0;
  for (let i = 0; i < lower.length && j < qs.length; i++) {
    if (lower[i] !== qs[j]) continue;
    score += 5;
    if (i === last + 1) score += 8;
    if (isBoundary(chars[i - 1], chars[i])) score += 6;
    indices.push(i);
    last = i;
    j++;
  }
  if (j < qs.length) return null;
  return { score: score - (indices[0] ?? 0) * 0.5, indices };
}

/** 在名称和别名里取最好的一个匹配，高亮只对名称有意义 */
export function bestMatch(label: string, keywords: string[], query: string): Match | null {
  const m = fuzzyMatch(label, query);
  let best = m;
  for (const k of keywords) {
    const km = fuzzyMatch(k, query);
    if (km && (!best || km.score - 5 > best.score)) best = { score: km.score - 5, indices: m?.indices ?? [] };
  }
  return best;
}
