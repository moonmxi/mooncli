// 左侧标签栏：可拖动缩放、四档宽度、收起和抽屉（docs/06 §1.1、§1.2，docs/08 §7）。
import { useRef, useState } from "react";
import { SIDEBAR, dragPreview, dragWidth, keyboardWidth, releaseWidth, toggleCollapsed, type Density } from "../layout/layout";
import { useSettings } from "../state/settings";
import { listView, orderedTabs, useStore, type Tab } from "../state/store";
import { displayTitle, metaLine } from "../state/title";
import { useUi } from "../state/ui";
import { Icon, kindIcon } from "./Icons";
import { TabStatus } from "./TabStatus";
import type { TabActions } from "./tabMenu";

interface Props {
  width: number;
  density: Density;
  mode: "docked" | "drawer";
  drawerOpen: boolean;
  uiScale: number;
  now: number;
  actions: TabActions;
  onCycleTheme(): void;
}

export function Sidebar({ width, density, mode, drawerOpen, uiScale, now, actions, onCycleTheme }: Props) {
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const activate = useStore((s) => s.activate);
  const openSettings = useUi((s) => s.openSettings);
  const [drag, setDrag] = useState<{ width: number } | null>(null);
  /** 正在拖的标签页和停在了哪个标签页上 */
  const [moving, setMoving] = useState<{ id: string; over?: string } | null>(null);

  // 拖动排序：按住标签页移动超过几像素才算拖动（不影响点击）；松手时放到所停的标签页的位置。
  // 用指针事件自己做，不用 HTML5 拖放——Tauri 在 Windows 上会截走原生拖放事件。
  const startMove = (e: React.PointerEvent, id: string) => {
    if (e.button !== 0 || useStore.getState().editingId) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    let started = false;
    let over: string | undefined;
    const move = (ev: PointerEvent) => {
      if (!started && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 6) return;
      started = true;
      const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-tab-id]");
      over = el?.dataset.tabId;
      setMoving({ id, over });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (started && over && over !== id) useStore.getState().moveTab(id, over);
      setMoving(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const shownDensity = drag ? dragPreview(drag.width) : density;
  const shownWidth = drag ? drag.width : width;
  const pinnedCount = tabs.filter((t) => t.pinned).length;

  return (
    <nav
      className={`side mode-${mode}${drawerOpen ? " open" : ""}${drag ? " dragging" : ""}`}
      data-density={shownDensity}
      style={{ width: shownWidth }}
      aria-label="标签页"
      onContextMenu={(e) => {
        if (e.target !== e.currentTarget && !(e.target as HTMLElement).classList.contains("tabs-scroll")) return;
        e.preventDefault();
        useUi.getState().openMenu(e.clientX, e.clientY, actions.blankMenu());
      }}
    >
      <button type="button" className="newtab" onClick={() => actions.newTab()} title="新标签页 (Ctrl+Shift+T)">
        <Icon name="plus" />
        <span className="long">新标签页</span>
        <span className="short">新建</span>
        <kbd className="hint">Ctrl+Shift+T</kbd>
      </button>
      <div className="tabs-scroll" role="tablist" aria-orientation="vertical">
        {orderedTabs(tabs).map((t, i) => (
          <div
            key={t.id}
            data-tab-id={t.id}
            className={`${i === pinnedCount - 1 && pinnedCount < orderedTabs(tabs).length ? "pin-divider" : ""}${moving?.over === t.id && moving.id !== t.id ? " drop-target" : ""}`}
            onPointerDown={(e) => startMove(e, t.id)}
          >
            <TabItem tab={listView(tabs, t)} active={t.id === activeId} density={shownDensity} now={now} onClick={() => activate(t.id)} actions={actions} />
          </div>
        ))}
      </div>
      <div className="side-foot">
        <button type="button" className="foot-btn" onClick={() => openSettings()} title="设置 (Ctrl+,)">
          <Icon name="gear" />
          <span className="lbl">设置</span>
        </button>
        <button type="button" className="foot-btn" onClick={() => useUi.getState().setHistory(true)} title="历史命令 (Ctrl+Shift+R)">
          <Icon name="history" />
          <span className="lbl">历史</span>
        </button>
        <button type="button" className="foot-btn" onClick={onCycleTheme} title="换一个主题">
          <Icon name="palette" />
          <span className="lbl">主题</span>
        </button>
      </div>
      {mode === "docked" && <ResizeHandle width={width} uiScale={uiScale} onDrag={setDrag} />}
    </nav>
  );
}

export function TabItem({
  tab,
  active,
  density,
  now,
  onClick,
  actions,
}: {
  tab: Tab;
  active: boolean;
  density: Density;
  now: number;
  onClick(): void;
  actions: TabActions;
}) {
  const rename = useStore((s) => s.rename);
  const editing = useStore((s) => s.editingId === tab.id);
  const setEditing = useStore((s) => s.setEditing);
  const title = displayTitle(tab);
  const meta = metaLine(tab, density === "wide");
  const kind = tab.running?.kind ?? (tab.remote ? "ssh" : "pwsh");
  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={active ? 0 : -1}
      className={`tab${active ? " active" : ""}${tab.attention ? " attn" : ""}${tab.pinned ? " pinned" : ""}`}
      data-color={tab.color}
      title={density === "collapsed" || density === "compact" ? `${title}\n${metaLine(tab, true)}` : metaLine(tab, true)}
      onClick={onClick}
      onAuxClick={(e) => e.button === 1 && actions.close(tab.id)}
      onDoubleClick={() => density !== "collapsed" && setEditing(tab.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onClick();
        if (e.key === "F2") setEditing(tab.id);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        useUi.getState().openMenu(e.clientX, e.clientY, actions.tabMenu(tab));
      }}
    >
      <span className="ico">
        <Icon name={kindIcon(kind)} />
        {tab.pinned && density !== "collapsed" && <span className="pin-mark" aria-label="已置顶" />}
      </span>
      {editing && density !== "collapsed" ? (
        <input
          className="ttl-edit"
          defaultValue={tab.title.user ?? title}
          autoFocus
          aria-label="标签页名称"
          onFocus={(e) => e.currentTarget.select()}
          onBlur={(e) => {
            rename(tab.id, e.currentTarget.value);
            setEditing(undefined);
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setEditing(undefined);
          }}
          onClick={(e) => e.stopPropagation()}
        />
      ) : (
        <span className="ttl">{title}</span>
      )}
      <span className="stat-wrap">
        <TabStatus tab={tab} now={now} showDuration={density === "standard" || density === "wide"} />
      </span>
      <span className="meta">{meta}</span>
      {!tab.pinned && (
        <button
          type="button"
          className="tab-close"
          aria-label="关闭标签页"
          onClick={(e) => {
            e.stopPropagation();
            actions.close(tab.id);
          }}
        >
          <Icon name="close" size={12} />
        </button>
      )}
    </div>
  );
}

/** 拖动手柄：拖动改宽度，低于 120 松手收起；双击切换；Tab 选中后 ←/→ 调整、Enter 切换。 */
function ResizeHandle({ width, uiScale, onDrag }: { width: number; uiScale: number; onDrag(d: { width: number } | null): void }) {
  const update = useSettings((s) => s.update);
  const lastOpen = useStore((s) => s.lastOpenWidth);
  const remember = useStore((s) => s.rememberOpenWidth);
  const start = useRef<{ x: number; w: number } | null>(null);
  const [tip, setTip] = useState<string | null>(null);
  const setWidth = (w: number) => {
    remember(w);
    update((d) => (d.tabs.sidebar.width = w));
  };
  // 侧栏在界面尺寸放大后显示，鼠标移动的屏幕像素要换算回侧栏自己的宽度
  const dxOf = (e: React.PointerEvent) => (e.clientX - start.current!.x) / uiScale;

  return (
    <div
      className="handle"
      role="separator"
      aria-orientation="vertical"
      aria-label="拖动调整标签栏宽度，双击收起或展开"
      aria-valuemin={SIDEBAR.collapsed}
      aria-valuemax={SIDEBAR.max}
      aria-valuenow={width}
      tabIndex={0}
      onPointerDown={(e) => {
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, w: width };
      }}
      onPointerMove={(e) => {
        if (!start.current) return;
        const w = dragWidth(start.current.w, dxOf(e));
        onDrag({ width: w });
        setTip(w < SIDEBAR.snapBelow ? "松手收起" : `${Math.round(w)}px`);
      }}
      onPointerUp={(e) => {
        if (!start.current) return;
        const w = dragWidth(start.current.w, dxOf(e));
        start.current = null;
        onDrag(null);
        setTip(null);
        setWidth(releaseWidth(w));
      }}
      onDoubleClick={() => setWidth(toggleCollapsed(width, lastOpen))}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          e.preventDefault();
          setWidth(keyboardWidth(width, e.key));
        } else if (e.key === "Enter") {
          setWidth(toggleCollapsed(width, lastOpen));
        }
      }}
    >
      {tip && <span className="width-tip">{tip}</span>}
    </div>
  );
}
