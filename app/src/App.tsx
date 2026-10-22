import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { pickBackend } from "./backend";
import { Backdrop, useBackgroundUrl, useRenderSize } from "./components/Backdrop";
import { CommandPalette, type PaletteCommand } from "./components/CommandPalette";
import { HistoryPanel } from "./components/HistoryPanel";
import { AgentWidget } from "./components/AgentWidget";
import { ContextMenu } from "./components/ContextMenu";
import { CopyToast, Notice } from "./components/CopyToast";
import { SettingsPage } from "./components/settings/SettingsPage";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { makeTabActions } from "./components/tabMenu";
import { TitleBar } from "./components/TitleBar";
import { computeLayout, PANEL, SIDEBAR } from "./layout/layout";
import { keyHint, matchShortcut, type Action } from "./shortcuts";
import {
  chooseTheme,
  clamp,
  effectiveFontSize,
  effectiveUiScale,
  LIMITS,
  mergeSettings,
  pickTheme,
  setSettingsSaver,
  useSettings,
  zoomStep,
} from "./state/settings";
import { newTabId, useStore } from "./state/store";
import { useUi } from "./state/ui";
import { TerminalView, getTerminal } from "./terminal/TerminalView";
import { applyTheme, builtinThemes } from "./theme/theme";


function measureCharWidth(family: string, size: number): number {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return 8.43;
  ctx.font = `${size}px ${family}`;
  return ctx.measureText("M".repeat(20)).width / 20;
}

function useSystemDark() {
  const [dark, setDark] = useState(() => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false);
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const on = (e: MediaQueryListEvent) => setDark(e.matches);
    mq?.addEventListener("change", on);
    return () => mq?.removeEventListener("change", on);
  }, []);
  return dark;
}

export function App() {
  const backend = useMemo(pickBackend, []);
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const drawerOpen = useStore((s) => s.drawerOpen);
  const settings = useSettings((s) => s.settings);
  const loaded = useSettings((s) => s.loaded);
  const systemDark = useSystemDark();
  const themeName = pickTheme(settings.theme, systemDark);
  const theme = builtinThemes.find((t) => t.name === themeName) ?? builtinThemes[0];
  const ui = effectiveUiScale(settings);
  const fontSize = effectiveFontSize(settings);
  const mainRef = useRef<HTMLElement>(null);
  const bgSize = useRenderSize(mainRef);
  const bgUrl = useBackgroundUrl(settings.background.image, theme, backend, bgSize);

  const [winWidth, setWinWidth] = useState(window.innerWidth);
  const [now, setNow] = useState(Date.now());
  const [size, setSize] = useState("");
  const charWidth = useMemo(() => measureCharWidth(settings.font.family, fontSize), [settings.font.family, fontSize]);

  // 读设置；保存时出错给个提示
  useEffect(() => {
    backend
      .loadSettings()
      .then((v) => useSettings.getState().setAll(mergeSettings(v)))
      .catch(() => useSettings.getState().setAll(mergeSettings({})));
    setSettingsSaver((v) =>
      backend.saveSettings(v).catch((e) => useUi.getState().showNotice(`保存设置失败：${String(e)}`)),
    );
  }, [backend]);

  useEffect(() => applyTheme(theme), [theme]);

  useEffect(() => {
    const onResize = () => setWinWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    const focus = () => useStore.getState().setFocused(true);
    const blur = () => useStore.getState().setFocused(false);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    // 不要 WebView 自带的右键菜单（刷新、另存为、打印……）；文本框里保留，方便剪切粘贴
    const noMenu = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea")) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", noMenu);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
      window.removeEventListener("contextmenu", noMenu);
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

  const newTab = useCallback((cwd?: string) => {
    useStore.getState().addTab(newTabId(), { cwd });
  }, []);

  const closeTab = useCallback(
    (id: string, force = false) => {
      const s = useStore.getState();
      const tab = s.tabs.find((t) => t.id === id);
      if (tab?.pinned && !force) {
        useUi.getState().showNotice("置顶的标签页请右键关闭");
        return;
      }
      s.closeTab(id);
      if (s.tabs.length <= 1) {
        // 最后一个标签页关掉时：真窗口直接关闭，演示时重新开一个
        if (backend.kind === "tauri") backend.window.close();
        else newTab();
      }
    },
    [backend, newTab],
  );

  const copyText = useCallback(
    (text: string) => {
      backend.clipboardWrite(text).catch(() => {});
      if (useSettings.getState().settings.terminal.copyToast) {
        const at = useUi.getState().lastMenuAt;
        useUi.getState().showCopyToast(at.x, at.y, text);
      }
    },
    [backend],
  );

  const actions = useMemo(() => makeTabActions({ newTab, close: closeTab, copyText, theme }), [newTab, closeTab, copyText, theme]);

  const cycleTheme = useCallback(() => {
    const i = builtinThemes.findIndex((t) => t.name === themeName);
    const next = builtinThemes[(i + 1) % builtinThemes.length];
    useSettings.getState().update((d) => (d.theme = chooseTheme(d.theme, next.name, next.kind)));
    useUi.getState().showNotice(`主题：${next.displayName}`);
  }, [themeName]);

  const zoom = useCallback((dir: 1 | -1 | 0) => {
    useSettings.getState().update((d) => (d.zoom = dir === 0 ? 1 : zoomStep(d.zoom, dir)));
    useUi.getState().showNotice(`缩放 ${Math.round(useSettings.getState().settings.zoom * 100)}%`);
  }, []);

  const fontStep = useCallback((dir: 1 | -1) => {
    useSettings.getState().update((d) => (d.font.size = clamp(d.font.size + dir, LIMITS.fontSize.min, LIMITS.fontSize.max)));
    useUi.getState().showNotice(`终端字号 ${useSettings.getState().settings.font.size}`);
  }, []);

  // 命令面板、设置、右键菜单关掉后，把键盘焦点还给当前终端，否则接着敲的字会丢
  useEffect(
    () =>
      useUi.subscribe((now, before) => {
        const wasOpen = before.paletteOpen || before.historyOpen || before.settings.open || !!before.menu;
        const isOpen = now.paletteOpen || now.historyOpen || now.settings.open || !!now.menu;
        if (wasOpen && !isOpen) {
          requestAnimationFrame(() => {
            // 改名输入框等自己拿了焦点的元素不抢
            if (document.activeElement && document.activeElement.matches("input, textarea:not(.xterm-helper-textarea)")) return;
            getTerminal(useStore.getState().activeId ?? "")?.term.focus();
          });
        }
      }),
    [],
  );

  // 第一个标签页：等设置读完再开，这样 shell 路径、集成开关等设置能生效
  const started = useRef(false);
  useEffect(() => {
    if (loaded && !started.current) {
      started.current = true;
      if (useStore.getState().tabs.length === 0) newTab();
    }
  }, [loaded, newTab]);

  const layout = computeLayout({
    windowWidth: winWidth,
    charWidth,
    tabsPosition: settings.tabs.position,
    sidebarWidth: settings.tabs.sidebar.width,
    autoCollapse: settings.tabs.sidebar.autoCollapse,
    drawerOpen,
    panelOpen: false,
    panelPinned: false,
    panelWidth: PANEL.default,
    uiScale: ui,
  });
  const layoutRef = useRef(layout);
  layoutRef.current = layout;

  // 所有能绑快捷键的操作（shortcuts.ts 的 ActionId），快捷键和命令面板都走这里
  const runAction = useCallback(
    (a: Action) => {
      const s = useStore.getState();
      const u = useUi.getState();
      const cur = s.tabs.find((t) => t.id === s.activeId);
      switch (a.type) {
        case "newTab":
          return newTab();
        case "duplicateTab":
          return newTab(cur?.cwd);
        case "closeTab":
          return s.activeId && closeTab(s.activeId);
        case "nextTab":
          return s.cycle(1);
        case "prevTab":
          return s.cycle(-1);
        case "gotoTab":
          return s.activateIndex(a.index);
        case "renameTab":
          return s.activeId && s.setEditing(s.activeId);
        case "pinTab":
          return s.activeId && s.togglePin(s.activeId);
        case "toggleSidebar": {
          if (layoutRef.current.sidebar.mode === "drawer") return s.setDrawerOpen(!s.drawerOpen);
          const w = useSettings.getState().settings.tabs.sidebar.width;
          return useSettings.getState().update((d) => (d.tabs.sidebar.width = w <= SIDEBAR.collapsed ? s.lastOpenWidth : SIDEBAR.collapsed));
        }
        case "zoomIn":
          return zoom(1);
        case "zoomOut":
          return zoom(-1);
        case "zoomReset":
          return zoom(0);
        case "fontBigger":
          return fontStep(1);
        case "fontSmaller":
          return fontStep(-1);
        case "settings":
          return u.settings.open ? u.closeSettings() : u.openSettings();
        case "palette":
          return u.setPalette(!u.paletteOpen);
        case "history":
          return u.setHistory(!u.historyOpen);
        case "clearScreen":
          return getTerminal(s.activeId ?? "")?.term.clear();
        case "cycleTheme":
          return cycleTheme();
        case "copyCwd":
          return cur?.cwd && copyText(cur.cwd);
      }
    },
    [newTab, closeTab, zoom, fontStep, cycleTheme, copyText],
  );
  const runRef = useRef(runAction);
  runRef.current = runAction;

  // 终端级快捷键：捕获阶段处理，这些键不会传给 shell。设置里正在录快捷键时不处理
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (useUi.getState().recordingKey) return;
      const a = matchShortcut(e, useSettings.getState().settings.keys);
      if (!a) return;
      e.preventDefault();
      e.stopPropagation();
      runRef.current(a);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const activeTab = tabs.find((t) => t.id === activeId);

  const keys = settings.keys;
  const commands: PaletteCommand[] = useMemo(() => {
    const st = () => useStore.getState();
    const cur = () => st().tabs.find((t) => t.id === st().activeId);
    const list: PaletteCommand[] = [
      { id: "new", group: "操作", label: "新建标签页", icon: "plus", hint: keyHint("newTab", keys), keywords: ["new tab"], run: () => newTab() },
      { id: "dup", group: "操作", label: "复制当前标签页（同一目录）", icon: "duplicate", hint: keyHint("duplicateTab", keys), keywords: ["duplicate"], run: () => newTab(cur()?.cwd) },
      { id: "close", group: "操作", label: "关闭当前标签页", icon: "close", hint: keyHint("closeTab", keys), keywords: ["close"], run: () => st().activeId && closeTab(st().activeId!) },
      { id: "rename", group: "操作", label: "重命名当前标签页", icon: "edit", hint: keyHint("renameTab", keys) ?? "F2", keywords: ["rename"], run: () => st().activeId && st().setEditing(st().activeId) },
      { id: "pin", group: "操作", label: "置顶 / 取消置顶当前标签页", icon: "pin", hint: keyHint("pinTab", keys), keywords: ["pin"], run: () => st().activeId && st().togglePin(st().activeId!) },
      { id: "copycwd", group: "操作", label: "复制当前目录路径", icon: "folder", hint: keyHint("copyCwd", keys), keywords: ["copy path", "pwd"], run: () => cur()?.cwd && copyText(cur()!.cwd!) },
      { id: "clear", group: "操作", label: "清除屏幕", icon: "eraser", hint: keyHint("clearScreen", keys), keywords: ["clear", "cls"], run: () => getTerminal(st().activeId ?? "")?.term.clear() },
      { id: "history", group: "操作", label: "历史命令", icon: "history", hint: keyHint("history", keys), keywords: ["history", "recent commands"], run: () => useUi.getState().setHistory(true) },
      { id: "settings", group: "操作", label: "打开设置", icon: "gear", hint: keyHint("settings", keys), keywords: ["settings", "preferences"], run: () => useUi.getState().openSettings() },
      { id: "zin", group: "缩放", label: "放大", icon: "zoom", hint: keyHint("zoomIn", keys), keywords: ["zoom in"], run: () => zoom(1) },
      { id: "zout", group: "缩放", label: "缩小", icon: "zoom", hint: keyHint("zoomOut", keys), keywords: ["zoom out"], run: () => zoom(-1) },
      { id: "z0", group: "缩放", label: "恢复默认缩放", icon: "zoom", hint: keyHint("zoomReset", keys), keywords: ["reset zoom"], run: () => zoom(0) },
      { id: "fin", group: "缩放", label: "终端字号加大", icon: "text", hint: keyHint("fontBigger", keys) ?? "Ctrl+滚轮", keywords: ["font bigger"], run: () => fontStep(1) },
      { id: "fout", group: "缩放", label: "终端字号减小", icon: "text", hint: keyHint("fontSmaller", keys) ?? "Ctrl+滚轮", keywords: ["font smaller"], run: () => fontStep(-1) },
      { id: "font", group: "缩放", label: "字体与缩放设置", icon: "text", keywords: ["font", "ui scale"], run: () => useUi.getState().openSettings("font") },
      {
        id: "pos",
        group: "布局",
        label: "标签栏放到顶部 / 左侧",
        icon: "layout",
        keywords: ["tabs position"],
        run: () => useSettings.getState().update((d) => (d.tabs.position = d.tabs.position === "left" ? "top" : "left")),
      },
      { id: "bg", group: "外观", label: "更换背景图", icon: "image", keywords: ["background"], run: () => useUi.getState().openSettings("appearance") },
      { id: "diag", group: "操作", label: "查看诊断信息", icon: "info", keywords: ["diagnostics", "debug"], run: () => useUi.getState().openSettings("shell") },
      ...builtinThemes.map((t) => ({
        id: `theme-${t.name}`,
        group: "外观",
        label: `主题：${t.displayName}`,
        icon: "palette",
        keywords: ["theme", t.name],
        run: () => useSettings.getState().update((d) => (d.theme = chooseTheme(d.theme, t.name, t.kind))),
      })),
    ];
    for (const t of tabs) {
      list.push({
        id: `tab-${t.id}`,
        group: "标签页",
        label: `切换到：${t.title.user ?? t.title.program ?? (t.cwd?.split(/[\\/]/).filter(Boolean).pop() || t.profile)}`,
        icon: t.running?.kind ?? "pwsh",
        keywords: [t.cwd ?? ""],
        run: () => useStore.getState().activate(t.id),
      });
    }
    return list;
  }, [tabs, newTab, closeTab, copyText, zoom, fontStep, keys]);

  const sideDocked = layout.sidebar.mode === "docked";
  const chrome = Math.round(40 * ui);
  const foot = Math.round(26 * ui);

  return (
    <div
      className="win"
      data-bp={layout.bp}
      data-theme-kind={theme.kind}
      style={
        {
          "--ui": ui,
          "--chrome-h": `${chrome}px`,
          "--foot-h": `${foot}px`,
          gridTemplateRows: `${chrome}px minmax(0, 1fr) ${foot}px`,
          gridTemplateColumns: `${sideDocked ? `${layout.sidebar.px}px` : "0px"} minmax(0, 1fr)`,
        } as React.CSSProperties
      }
    >
      <TitleBar backend={backend} bp={layout.bp} showMenu={layout.sidebar.mode === "drawer"} topTabs={settings.tabs.position === "top"} now={now} actions={actions} />
      {layout.sidebar.mode !== "none" && (
        <Sidebar
          width={layout.sidebar.width}
          density={layout.sidebar.density}
          mode={layout.sidebar.mode === "drawer" ? "drawer" : "docked"}
          drawerOpen={layout.sidebar.drawerOpen}
          uiScale={ui}
          now={now}
          actions={actions}
          onCycleTheme={cycleTheme}
        />
      )}
      {layout.sidebar.drawerOpen && <div className="drawer-backdrop" onClick={() => useStore.getState().setDrawerOpen(false)} />}
      <main ref={mainRef} className={`main${bgUrl ? " has-bg" : ""}`}>
        <Backdrop bg={settings.background} url={bgUrl} theme={theme} />
        {tabs.map((t) => (
          <TerminalView key={t.id} id={t.id} active={t.id === activeId} theme={theme} backend={backend} transparent={!!bgUrl} />
        ))}
      </main>
      <StatusBar tab={activeTab} bp={layout.bp} size={size} backendKind={backend.kind} zoom={settings.zoom} />
      <AgentWidget backend={backend} />
      <SettingsPage backend={backend} systemDark={systemDark} />
      <CommandPalette commands={commands} backend={backend} />
      <HistoryPanel backend={backend} onNewTab={newTab} />
      <ContextMenu />
      <CopyToast />
      <Notice />
    </div>
  );
}
