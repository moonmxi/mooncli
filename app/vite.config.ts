import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri 开发时固定用 1420 端口；界面也可以单独在浏览器里打开（用演示后端）
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // 主题文件在仓库根目录的 design/ 和 resources/ 下
    fs: { allow: [".."] },
  },
  // xterm.js 加 WebGL 渲染器本身就有几百 KB，桌面应用从本地加载，不需要拆包
  build: { target: "es2022", sourcemap: true, chunkSizeWarningLimit: 2000 },
  test: { environment: "node" },
});
