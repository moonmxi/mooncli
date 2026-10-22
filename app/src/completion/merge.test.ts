import { describe, expect, it } from "vitest";
import { keystrokes, mergeCandidates, parsePwsh, wordStart } from "./merge";
import { specCandidates } from "./specs";

describe("specCandidates", () => {
  it("命令名、子命令、参数", () => {
    expect(specCandidates("gi").map((c) => c.text)).toContain("git");
    expect(specCandidates("git ch").map((c) => c.text)).toEqual(expect.arrayContaining(["checkout", "cherry-pick"]));
    expect(specCandidates("git commit --a").map((c) => c.text)).toEqual(expect.arrayContaining(["--all", "--amend"]));
    expect(specCandidates("docker compose u").map((c) => c.text)).toEqual(["up"]);
    // 别名
    expect(specCandidates("ls -Rec").map((c) => c.text)).toEqual(["-Recurse"]);
    expect(specCandidates("unknown-tool x")).toEqual([]);
    expect(specCandidates("")).toEqual([]);
  });
});

describe("wordStart", () => {
  it("按空白分词，引号里的空格不算", () => {
    expect(wordStart("git ch", 6)).toBe(4);
    expect(wordStart('cd "Program Fi', 14)).toBe(3);
    expect(wordStart("ls", 2)).toBe(0);
  });
});

describe("mergeCandidates", () => {
  const pwsh = parsePwsh({
    line: "git ch",
    cursor: 6,
    start: 4,
    len: 2,
    items: [{ t: "checkout", l: "checkout", k: "ParameterValue" }],
  });
  it("pwsh 的项带上说明书的中文说明，同样的插入只留一个，历史排后面", () => {
    const list = mergeCandidates({ line: "git ch", cursor: 6, pwsh, history: [{ text: "git checkout main", count: 3 }] });
    const checkout = list.filter((c) => c.insert === "checkout");
    expect(checkout).toHaveLength(1);
    expect(checkout[0].source).toBe("pwsh");
    expect(checkout[0].desc).toBe("切换分支或恢复文件");
    expect(list.at(-1)).toMatchObject({ source: "history", start: 0, end: 6, desc: "历史 · 3 次" });
  });
  it("输入行对不上时不用过期的 pwsh 结果", () => {
    const list = mergeCandidates({ line: "git che", cursor: 7, pwsh });
    expect(list.every((c) => c.source !== "pwsh")).toBe(true);
  });
});

describe("keystrokes", () => {
  it("替换光标前的词：退格再输入", () => {
    expect(keystrokes("git ch", 6, { start: 4, end: 6, insert: "checkout" })).toBe("\x7f\x7fcheckout");
  });
  it("替换段在光标后面：先右移", () => {
    expect(keystrokes("git chx y", 6, { start: 4, end: 7, insert: "checkout" })).toBe("\x1b[C\x7f\x7f\x7fcheckout");
  });
  it("整行替换且光标不在行尾：Delete 清掉后半段，不按 End", () => {
    expect(keystrokes("git st x", 6, { start: 0, end: 8, insert: "git status" })).toBe("\x1b[3~\x1b[3~" + "\x7f".repeat(6) + "git status");
  });
});
