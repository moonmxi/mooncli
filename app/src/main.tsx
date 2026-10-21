import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

// 不用 StrictMode：它会在开发时把终端组件挂载两次，每个标签页白白多起一个 pwsh
createRoot(document.getElementById("root")!).render(<App />);
