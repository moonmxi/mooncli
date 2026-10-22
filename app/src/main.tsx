import { createRoot } from "react-dom/client";
import { App } from "./App";
import { AgentWindowApp } from "./AgentWindowApp";
import "./styles.css";

// 独立的 Agent 用量小窗是另一个 Tauri 窗口，创建时注入 __MOONCLI_VIEW（见 src-tauri/src/agent.rs 的
// agent_window_toggle）；浏览器里调界面时用 ?view=agent
const view = (window as unknown as { __MOONCLI_VIEW?: string }).__MOONCLI_VIEW ?? new URLSearchParams(location.search).get("view");

// 不用 StrictMode：它会在开发时把终端组件挂载两次，每个标签页白白多起一个 pwsh
createRoot(document.getElementById("root")!).render(view === "agent" ? <AgentWindowApp /> : <App />);
