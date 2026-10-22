// 编译 mooncli-bridge 并放到 Tauri 的 sidecar 位置（src-tauri/binaries/mooncli-bridge-<目标三元组>.exe）。
// 打包时 Tauri 把它装在 mooncli.exe 旁边，名字是 mooncli-bridge.exe（docs/07 第 3 节）。
// 用法：node scripts/build-bridge.mjs [--release]
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const release = process.argv.includes("--release");
const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const triple = execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const ext = process.platform === "win32" ? ".exe" : "";

execFileSync("cargo", ["build", "-p", "mooncli-agent", "--bin", "mooncli-bridge", ...(release ? ["--release"] : [])], {
  cwd: root,
  stdio: "inherit",
});
const from = join(root, "target", release ? "release" : "debug", `mooncli-bridge${ext}`);
const dir = join(root, "app", "src-tauri", "binaries");
mkdirSync(dir, { recursive: true });
copyFileSync(from, join(dir, `mooncli-bridge-${triple}${ext}`));
console.log(`mooncli-bridge → binaries/mooncli-bridge-${triple}${ext}`);
