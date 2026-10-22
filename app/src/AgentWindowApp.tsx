// 独立的 Agent 用量小窗（docs/07 第 5 节的第四种形态）：一个置顶的小窗口，只有用量卡片。
// 它是单独的 WebView，自己读设置、自己轮询；没有标签页可跟随，显示最近活动的会话。
import { useEffect, useMemo } from "react";
import { startUsagePolling } from "./agent/store";
import { pickBackend } from "./backend";
import { AgentWindowView } from "./components/AgentWidget";
import { mergeSettings, pickTheme, useSettings } from "./state/settings";
import { applyTheme, builtinThemes } from "./theme/theme";

export function AgentWindowApp() {
  const backend = useMemo(pickBackend, []);
  const settings = useSettings((s) => s.settings);
  const loaded = useSettings((s) => s.loaded);
  const dark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  const theme = builtinThemes.find((t) => t.name === pickTheme(settings.theme, dark)) ?? builtinThemes[0];

  useEffect(() => {
    const load = () =>
      backend
        .loadSettings()
        .then((v) => useSettings.getState().setAll(mergeSettings(v)))
        .catch(() => useSettings.getState().setAll(mergeSettings({})));
    load();
    // 主窗口改了主题、阈值时跟着变
    const t = setInterval(load, 10_000);
    return () => clearInterval(t);
  }, [backend]);
  useEffect(() => applyTheme(theme), [theme]);
  useEffect(() => (loaded ? startUsagePolling(backend) : undefined), [loaded, backend]);

  return <AgentWindowView backend={backend} onClose={() => backend.window.close()} />;
}
