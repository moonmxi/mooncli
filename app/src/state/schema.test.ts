import { readFileSync, writeFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { settingsSchema } from "./schema";

const FILE = new URL("../../../schema/settings.schema.json", import.meta.url);

describe("settings.schema.json", () => {
  it("和默认设置生成的一致", () => {
    const want = `${JSON.stringify(settingsSchema(), null, 2)}\n`;
    if (process.env.UPDATE_SCHEMA) writeFileSync(FILE, want);
    const have = readFileSync(FILE, "utf8").replace(/\r\n/g, "\n");
    expect(have, "设置结构变了：运行 UPDATE_SCHEMA=1 pnpm test 更新 schema/settings.schema.json").toBe(want);
  });
});
