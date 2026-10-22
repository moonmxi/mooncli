// 独立的 Agent 额度小窗（docs/07 第 5 节）：一个置顶的小窗口，只有额度卡片。
// 它是单独的 WebView，自己读设置、自己轮询；它没有标签页，所以只显示两个 Agent 的额度，不显示上下文。
import { useEffect, useMemo } from "react";
import { startUsagePolling } from "./agent/store";
import { pickBackend } from "./backend";
import { AgentWindowView } from "./components/AgentWidget";
import { mergeSettings, pickTheme, useSettings } from "./state/settings";
import { applyTheme, findTheme } from "./theme/theme";

export function AgentWindowApp() {
  const backend = useMemo(pickBackend, []);
  const settings = useSettings((s) => s.settings);
  const loaded = useSettings((s) => s.loaded);
  const dark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  const theme = findTheme(pickTheme(settings.theme, dark), settings.customThemes);

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
