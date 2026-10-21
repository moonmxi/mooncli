// 自己画的右键菜单，替代 WebView 自带的"刷新 / 另存为 / 打印"。
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { clampToViewport, useUi, type MenuItem } from "../state/ui";
import { Icon } from "./Icons";

export function ContextMenu() {
  const menu = useUi((s) => s.menu);
  const close = useUi((s) => s.closeMenu);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: -9999, y: -9999 });
  const [focus, setFocus] = useState(-1);

  const items = (menu?.items ?? []).map((e, i) => ({ e, i }));
  const selectable = items.filter((x) => x.e !== "separator" && !x.e.disabled && !x.e.swatches).map((x) => x.i);

  useLayoutEffect(() => {
    if (!menu || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    setPos(clampToViewport(menu.x, menu.y, r.width, r.height, innerWidth, innerHeight));
    setFocus(-1);
    ref.current.focus();
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) close();
    };
    const onBlur = () => close();
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("blur", onBlur);
    window.addEventListener("resize", onBlur);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("resize", onBlur);
    };
  }, [menu, close]);

  if (!menu) return null;

  const run = (item: MenuItem) => {
    close();
    item.onSelect?.();
  };

  return (
    <div
      ref={ref}
      className="ctx-menu"
      role="menu"
      tabIndex={-1}
      style={{ left: pos.x, top: pos.y }}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") return close();
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const cur = selectable.indexOf(focus);
          const next = e.key === "ArrowDown" ? cur + 1 : cur - 1;
          setFocus(selectable[(next + selectable.length) % selectable.length]);
        }
        if (e.key === "Enter" && focus >= 0) {
          const it = menu.items[focus];
          if (it !== "separator") run(it);
        }
      }}
    >
      <div className="ctx-inner ui-zoom">
      {items.map(({ e, i }) =>
        e === "separator" ? (
          <div key={i} className="ctx-sep" role="separator" />
        ) : e.swatches ? (
          <div key={i} className="ctx-swatches" role="group" aria-label={e.label}>
            <span className="ctx-label">{e.label}</span>
            {e.swatches.map((s) => (
              <button
                key={s.key}
                type="button"
                className={`swatch${s.active ? " active" : ""}`}
                style={{ background: s.color }}
                title={s.label}
                aria-label={s.label}
                aria-pressed={s.active}
                onClick={() => {
                  close();
                  s.onPick();
                }}
              />
            ))}
          </div>
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            className={`ctx-item${e.danger ? " danger" : ""}${focus === i ? " focus" : ""}`}
            disabled={e.disabled}
            onMouseEnter={() => setFocus(i)}
            onClick={() => run(e)}
          >
            <span className="ctx-icon">{e.icon && <Icon name={e.icon} size={14} />}</span>
            <span className="ctx-label">{e.label}</span>
            {e.hint && <kbd>{e.hint}</kbd>}
          </button>
        ),
      )}
      </div>
    </div>
  );
}
