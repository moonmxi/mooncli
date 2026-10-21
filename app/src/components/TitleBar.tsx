// 自定义标题栏：品牌、（顶部布局时的）标签、命令面板入口、窗口按钮。空白处可拖动窗口。
import type { Backend } from "../backend/types";
import type { Breakpoint } from "../layout/layout";
import { useSettings } from "../state/settings";
import { orderedTabs, useStore } from "../state/store";
import { displayTitle } from "../state/title";
import { useUi } from "../state/ui";
import { Icon, kindIcon } from "./Icons";
import type { TabActions } from "./tabMenu";
import { TabStatus } from "./TabStatus";

interface Props {
  backend: Backend;
  bp: Breakpoint;
  showMenu: boolean;
  topTabs: boolean;
  now: number;
  actions: TabActions;
}

export function TitleBar({ backend, bp, showMenu, topTabs, now, actions }: Props) {
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const activate = useStore((s) => s.activate);
  const drawerOpen = useStore((s) => s.drawerOpen);
  const setDrawerOpen = useStore((s) => s.setDrawerOpen);
  const position = useSettings((s) => s.settings.tabs.position);
  const update = useSettings((s) => s.update);
  const setPalette = useUi((s) => s.setPalette);
  const narrow = bp === "md" || bp === "sm";

  return (
    <header className="titlebar" data-tauri-drag-region>
      {showMenu && (
        <button type="button" className="icon-btn" aria-label="显示标签栏" aria-expanded={drawerOpen} onClick={() => setDrawerOpen(!drawerOpen)}>
          <Icon name="menu" />
        </button>
      )}
      <div className="brand" data-tauri-drag-region>
        <img className="brand-mark" src="/mooncli.svg" width={20} height={20} alt="" />
        {!(topTabs || bp === "sm") && <span className="name">mooncli</span>}
      </div>
      {topTabs && (
        <div className="toptabs" role="tablist">
          {orderedTabs(tabs).map((t) => (
            <div
              key={t.id}
              role="tab"
              aria-selected={t.id === activeId}
              className={`toptab${t.id === activeId ? " active" : ""}${t.attention ? " attn" : ""}${t.pinned ? " pinned" : ""}`}
              data-color={t.color}
              onClick={() => activate(t.id)}
              onAuxClick={(e) => e.button === 1 && actions.close(t.id)}
              onContextMenu={(e) => {
                e.preventDefault();
                useUi.getState().openMenu(e.clientX, e.clientY, actions.tabMenu(t));
              }}
            >
              <Icon name={kindIcon(t.running?.kind ?? (t.remote ? "ssh" : "pwsh"))} />
              {!t.pinned && <span className="ttl">{t.title.user ?? displayTitle(t)}</span>}
              <TabStatus tab={t} now={now} showDuration={false} />
            </div>
          ))}
          <button type="button" className="icon-btn" aria-label="新标签页" onClick={() => actions.newTab()}>
            <Icon name="plus" />
          </button>
        </div>
      )}
      <div className="spacer" data-tauri-drag-region />
      <button type="button" className={`cmdbox${narrow || topTabs ? " compact" : ""}`} title="命令面板 (Ctrl+Shift+P)" onClick={() => setPalette(true)}>
        <Icon name="search" />
        <span className="t">搜索操作或历史命令</span>
        <kbd>Ctrl+Shift+P</kbd>
      </button>
      <button
        type="button"
        className="icon-btn"
        title={position === "left" ? "标签栏放到顶部" : "标签栏放到左侧"}
        aria-label="切换标签栏位置"
        onClick={() => update((d) => (d.tabs.position = d.tabs.position === "left" ? "top" : "left"))}
      >
        <Icon name="layout" />
      </button>
      <div className="sysbtns">
        <button type="button" aria-label="最小化" onClick={backend.window.minimize}>
          ─
        </button>
        <button type="button" aria-label="最大化" onClick={backend.window.toggleMaximize}>
          ☐
        </button>
        <button type="button" className="close" aria-label="关闭窗口" onClick={backend.window.close}>
          ✕
        </button>
      </div>
    </header>
  );
}
