// 标签页右键菜单的内容（docs/06 §6）。顶部和左侧两种标签栏共用。
import { TAB_COLORS, orderedTabs, useStore, type Tab, type TabColor } from "../state/store";
import { useSettings } from "../state/settings";
import type { MenuEntry } from "../state/ui";
import type { Theme } from "../theme/theme";

const COLOR_NAMES: Record<TabColor, string> = {
  red: "红",
  yellow: "黄",
  green: "绿",
  cyan: "青",
  blue: "蓝",
  magenta: "紫",
};

export interface TabActions {
  newTab(cwd?: string): void;
  close(id: string): void;
  tabMenu(tab: Tab): MenuEntry[];
  blankMenu(): MenuEntry[];
}

export function makeTabActions(opts: {
  newTab(cwd?: string): void;
  close(id: string, force?: boolean): void;
  copyText(text: string): void;
  theme: Theme;
}): TabActions {
  const st = () => useStore.getState();
  return {
    newTab: opts.newTab,
    close: (id) => opts.close(id, true),
    tabMenu(tab) {
      const order = orderedTabs(st().tabs);
      const i = order.findIndex((t) => t.id === tab.id);
      const after = order.slice(i + 1).filter((t) => !t.pinned);
      const others = order.filter((t) => t.id !== tab.id && !t.pinned);
      const top = useSettings.getState().settings.tabs.position === "top";
      return [
        { label: "重命名", icon: "edit", hint: "F2", onSelect: () => st().setEditing(tab.id) },
        { label: tab.pinned ? "取消置顶" : "置顶", icon: "pin", onSelect: () => st().togglePin(tab.id) },
        { label: "复制标签页", icon: "duplicate", hint: "同一目录", onSelect: () => opts.newTab(tab.cwd) },
        {
          label: "颜色",
          swatches: [
            ...TAB_COLORS.map((c) => ({
              key: c,
              color: opts.theme.terminal[c],
              active: tab.color === c,
              label: COLOR_NAMES[c],
              onPick: () => st().setColor(tab.id, tab.color === c ? undefined : c),
            })),
          ],
        },
        { label: "复制当前目录路径", icon: "folder", disabled: !tab.cwd, onSelect: () => tab.cwd && opts.copyText(tab.cwd) },
        ...(tab.title.user ? [{ label: "恢复自动标题", icon: "edit", onSelect: () => st().rename(tab.id, undefined) }] : []),
        "separator",
        { label: "关闭", icon: "close", hint: tab.pinned ? undefined : "Ctrl+Shift+W", onSelect: () => opts.close(tab.id, true) },
        { label: "关闭其他标签页", disabled: others.length === 0, onSelect: () => others.forEach((t) => opts.close(t.id, true)) },
        { label: top ? "关闭右侧的标签页" : "关闭下方的标签页", disabled: after.length === 0, onSelect: () => after.forEach((t) => opts.close(t.id, true)) },
      ];
    },
    blankMenu() {
      const s = useSettings.getState().settings;
      const width = s.tabs.sidebar.width;
      return [
        { label: "新标签页", icon: "plus", hint: "Ctrl+Shift+T", onSelect: () => opts.newTab() },
        "separator",
        {
          label: s.tabs.position === "left" ? "标签栏放到顶部" : "标签栏放到左侧",
          icon: "layout",
          onSelect: () => useSettings.getState().update((d) => (d.tabs.position = d.tabs.position === "left" ? "top" : "left")),
        },
        ...(s.tabs.position === "left"
          ? [
              {
                label: width <= 52 ? "展开标签栏" : "收起标签栏",
                hint: "Ctrl+Shift+B",
                onSelect: () =>
                  useSettings.getState().update((d) => (d.tabs.sidebar.width = width <= 52 ? useStore.getState().lastOpenWidth : 52)),
              },
            ]
          : []),
      ];
    },
  };
}
