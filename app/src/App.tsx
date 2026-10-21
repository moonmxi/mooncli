import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { pickBackend } from "./backend";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import { computeLayout, PANEL } from "./layout/layout";
import { matchShortcut } from "./shortcuts";
import { useStore } from "./state/store";
import { TERMINAL_FONT, TERMINAL_FONT_SIZE, TerminalView, getTerminal } from "./terminal/TerminalView";
import { applyTheme, builtinThemes } from "./theme/theme";

let tabCounter = 0;

function measureCharWidth(): number {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return 8.43;
  ctx.font = `${TERMINAL_FONT_SIZE}px ${TERMINAL_FONT}`;
  return ctx.measureText("M".repeat(20)).width / 20;
}

export function App() {
  const backend = useMemo(pickBackend, []);
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const themeName = useStore((s) => s.theme);
  const sidebarWidth = useStore((s) => s.sidebarWidth);
  const tabsPosition = useStore((s) => s.tabsPosition);
  const autoCollapse = useStore((s) => s.autoCollapse);
  const drawerOpen = useStore((s) => s.drawerOpen);
  const theme = builtinThemes.find((t) => t.name === themeName) ?? builtinThemes[0];

  const [winWidth, setWinWidth] = useState(window.innerWidth);
  const [charWidth, setCharWidth] = useState(8.43);
  const [now, setNow] = useState(Date.now());
  const [size, setSize] = useState("");

  useEffect(() => applyTheme(theme), [theme]);

  useEffect(() => {
    const onResize = () => setWinWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    setCharWidth(measureCharWidth());
    document.fonts?.ready.then(() => setCharWidth(measureCharWidth()));
    const focus = () => useStore.getState().setFocused(true);
    const blur = () => useStore.getState().setFocused(false);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
    };
  }, []);

  useEffect(() => {
    const t = setInterval(() => {
      setNow(Date.now());
      const term = getTerminal(useStore.getState().activeId ?? "")?.term;
      if (term) setSize(`${term.cols} × ${term.rows}`);
    }, 1000);
    return () => clearInterval(t);
  }, []);

  const newTab = useCallback(() => {
    tabCounter += 1;
    useStore.getState().addTab(`tab-${Date.now().toString(36)}-${tabCounter}`);
  }, []);

  const closeTab = useCallback(
    (id: string) => {
      const s = useStore.getState();
      s.closeTab(id);
      if (s.tabs.length <= 1) {
        // 最后一个标签页关掉时：真窗口直接关闭，演示时重新开一个
        if (backend.kind === "tauri") backend.window.close();
        else newTab();
      }
    },
    [backend, newTab],
  );

  const cycleTheme = useCallback(() => {
    const i = builtinThemes.findIndex((t) => t.name === useStore.getState().theme);
    useStore.getState().setTheme(builtinThemes[(i + 1) % builtinThemes.length].name);
  }, []);

  // 第一个标签页（StrictMode 下 effect 会跑两次，用 ref 挡住）
  const started = useRef(false);
  useEffect(() => {
    if (!started.current) {
      started.current = true;
      if (useStore.getState().tabs.length === 0) newTab();
    }
  }, [newTab]);

  // 终端级快捷键：捕获阶段处理，这些键不会传给 shell
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const a = matchShortcut(e);
      if (!a) return;
      e.preventDefault();
      e.stopPropagation();
      const s = useStore.getState();
      switch (a.type) {
        case "newTab":
          return newTab();
        case "closeTab":
          return s.activeId && closeTab(s.activeId);
        case "nextTab":
          return s.cycle(1);
        case "prevTab":
          return s.cycle(-1);
        case "gotoTab":
          return s.activateIndex(a.index);
        case "toggleSidebar": {
          if (s.drawerOpen || layoutRef.current.sidebar.mode === "drawer") return s.setDrawerOpen(!s.drawerOpen);
          return s.setSidebarWidth(s.sidebarWidth <= 52 ? s.lastOpenWidth : 52);
        }
        case "find":
          return; // M1 后半：搜索框
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [newTab, closeTab]);

  const layout = computeLayout({
    windowWidth: winWidth,
    charWidth,
    tabsPosition,
    sidebarWidth,
    autoCollapse,
    drawerOpen,
    panelOpen: false,
    panelPinned: false,
    panelWidth: PANEL.default,
  });
  const layoutRef = useRef(layout);
  layoutRef.current = layout;
  const sideDocked = layout.sidebar.mode === "docked";
  const activeTab = tabs.find((t) => t.id === activeId);

  return (
    <div
      className="win"
      data-bp={layout.bp}
      data-theme-kind={theme.kind}
      style={{ gridTemplateColumns: `${sideDocked ? `${layout.sidebar.width}px` : "0px"} minmax(0, 1fr)` }}
    >
      <TitleBar
        backend={backend}
        bp={layout.bp}
        showMenu={layout.sidebar.mode === "drawer"}
        topTabs={tabsPosition === "top"}
        now={now}
        onNewTab={newTab}
        onCloseTab={closeTab}
      />
      {layout.sidebar.mode !== "none" && (
        <Sidebar
          width={layout.sidebar.width}
          density={layout.sidebar.density}
          mode={layout.sidebar.mode === "drawer" ? "drawer" : "docked"}
          drawerOpen={layout.sidebar.drawerOpen}
          now={now}
          onNewTab={newTab}
          onCloseTab={closeTab}
          onCycleTheme={cycleTheme}
        />
      )}
      {layout.sidebar.drawerOpen && (
        <div className="drawer-backdrop" onClick={() => useStore.getState().setDrawerOpen(false)} />
      )}
      <main className="main">
        {tabs.map((t) => (
          <TerminalView key={t.id} id={t.id} active={t.id === activeId} theme={theme} backend={backend} />
        ))}
      </main>
      <StatusBar tab={activeTab} bp={layout.bp} size={size} backendKind={backend.kind} />
    </div>
  );
}
