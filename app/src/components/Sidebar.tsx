// 左侧标签栏：可拖动缩放、四档宽度、收起和抽屉（docs/06 §1.1、§1.2，docs/08 §7）。
import { useRef, useState } from "react";
import {
  SIDEBAR,
  dragPreview,
  dragWidth,
  keyboardWidth,
  releaseWidth,
  toggleCollapsed,
  type Density,
} from "../layout/layout";
import { useStore, type Tab } from "../state/store";
import { displayTitle, metaLine } from "../state/title";
import { Icon, kindIcon } from "./Icons";
import { TabStatus } from "./TabStatus";

interface Props {
  width: number;
  density: Density;
  mode: "docked" | "drawer";
  drawerOpen: boolean;
  now: number;
  onNewTab(): void;
  onCloseTab(id: string): void;
  onCycleTheme(): void;
}

export function Sidebar({ width, density, mode, drawerOpen, now, onNewTab, onCloseTab, onCycleTheme }: Props) {
  const tabs = useStore((s) => s.tabs);
  const activeId = useStore((s) => s.activeId);
  const activate = useStore((s) => s.activate);
  const [drag, setDrag] = useState<{ width: number } | null>(null);
  const shownDensity = drag ? dragPreview(drag.width) : density;
  const shownWidth = drag ? drag.width : width;

  return (
    <nav
      className={`side mode-${mode}${drawerOpen ? " open" : ""}${drag ? " dragging" : ""}`}
      data-density={shownDensity}
      style={{ width: shownWidth }}
      aria-label="标签页"
    >
      <button type="button" className="newtab" onClick={onNewTab} title="新标签页 (Ctrl+Shift+T)">
        <Icon name="plus" />
        <span className="long">新标签页</span>
        <span className="short">新建</span>
        <kbd className="hint">Ctrl+Shift+T</kbd>
      </button>
      <div className="tabs-scroll" role="tablist" aria-orientation="vertical">
        {tabs.map((t) => (
          <TabItem
            key={t.id}
            tab={t}
            active={t.id === activeId}
            density={shownDensity}
            now={now}
            onClick={() => activate(t.id)}
            onClose={() => onCloseTab(t.id)}
          />
        ))}
      </div>
      <div className="side-foot">
        <button type="button" className="foot-btn" title="设置（M3）" disabled>
          <Icon name="gear" />
          <span className="lbl">设置</span>
        </button>
        <button type="button" className="foot-btn" onClick={onCycleTheme} title="切换主题">
          <Icon name="palette" />
          <span className="lbl">主题</span>
        </button>
      </div>
      {mode === "docked" && <ResizeHandle width={width} onDrag={setDrag} />}
    </nav>
  );
}

function TabItem({
  tab,
  active,
  density,
  now,
  onClick,
  onClose,
}: {
  tab: Tab;
  active: boolean;
  density: Density;
  now: number;
  onClick(): void;
  onClose(): void;
}) {
  const rename = useStore((s) => s.rename);
  const [editing, setEditing] = useState(false);
  const ctx = { ...tab, running: tab.running };
  const title = displayTitle(ctx);
  const meta = metaLine(ctx, density === "wide");
  const kind = tab.running?.kind ?? (tab.remote ? "ssh" : "pwsh");
  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={active ? 0 : -1}
      className={`tab${active ? " active" : ""}${tab.attention ? " attn" : ""}`}
      title={density === "collapsed" || density === "compact" ? `${title}\n${metaLine(ctx, true)}` : metaLine(ctx, true)}
      onClick={onClick}
      onAuxClick={(e) => e.button === 1 && onClose()}
      onDoubleClick={() => density !== "collapsed" && setEditing(true)}
      onKeyDown={(e) => e.key === "Enter" && onClick()}
    >
      <span className="ico">
        <Icon name={kindIcon(kind)} />
      </span>
      {editing ? (
        <input
          className="ttl-edit"
          defaultValue={tab.title.user ?? title}
          autoFocus
          aria-label="标签页名称"
          onBlur={(e) => {
            rename(tab.id, e.currentTarget.value);
            setEditing(false);
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setEditing(false);
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
      <button
        type="button"
        className="tab-close"
        aria-label="关闭标签页"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  );
}

/** 拖动手柄：拖动改宽度，低于 120 松手收起；双击切换；Tab 选中后 ←/→ 调整、Enter 切换。 */
function ResizeHandle({ width, onDrag }: { width: number; onDrag(d: { width: number } | null): void }) {
  const setWidth = useStore((s) => s.setSidebarWidth);
  const lastOpen = useStore((s) => s.lastOpenWidth);
  const start = useRef<{ x: number; w: number } | null>(null);
  const [tip, setTip] = useState<string | null>(null);

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
        const w = dragWidth(start.current.w, e.clientX - start.current.x);
        onDrag({ width: w });
        setTip(w < SIDEBAR.snapBelow ? "松手收起" : `${Math.round(w)}px`);
      }}
      onPointerUp={(e) => {
        if (!start.current) return;
        const w = dragWidth(start.current.w, e.clientX - start.current.x);
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
