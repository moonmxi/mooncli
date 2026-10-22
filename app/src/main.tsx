import { createRoot } from "react-dom/client";
import { pickBackend } from "./backend";
import { App } from "./App";
import { AgentWindowApp } from "./AgentWindowApp";
import "./styles.css";

// 独立的 Agent 额度小窗是另一个 Tauri 窗口，创建时注入 __MOONCLI_VIEW（见 src-tauri/src/agent.rs 的
// agent_window_toggle）；浏览器里调界面时用 ?view=agent
const view = (window as unknown as { __MOONCLI_VIEW?: string }).__MOONCLI_VIEW ?? new URLSearchParams(location.search).get("view");

// 界面里没接住的错误记进本机日志（设置 → 关于 → 打包日志），不上传
const backend = pickBackend();
window.addEventListener("error", (e) => backend.logError(`${e.message} @ ${e.filename}:${e.lineno}:${e.colno}\n${e.error?.stack ?? ""}`));
window.addEventListener("unhandledrejection", (e) => backend.logError(`未处理的 Promise 拒绝：${e.reason?.stack ?? String(e.reason)}`));

// 不用 StrictMode：它会在开发时把终端组件挂载两次，每个标签页白白多起一个 pwsh
createRoot(document.getElementById("root")!).render(view === "agent" ? <AgentWindowApp /> : <App />);
