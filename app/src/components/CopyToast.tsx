// "已复制"小卡片：在右键处（或选区末尾）弹出，一个带弹性的出场、logo 升起、向上飘走的退场。
// 规格见 docs/05 §4。
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { clampToViewport, copySummary, useUi } from "../state/ui";

// 停留时间短一点：看到就够了，不要挡着后面的字
const STAY_MS = 900;

export function CopyToast() {
  const toast = useUi((s) => s.copyToast);
  const ref = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState<typeof toast>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    if (!toast) return;
    setShown(toast);
    setPos(null);
    setLeaving(false);
    const t1 = setTimeout(() => setLeaving(true), STAY_MS);
    const t2 = setTimeout(() => setShown(null), STAY_MS + 280);
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
    };
  }, [toast]);

  useLayoutEffect(() => {
    if (!shown || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    // 卡片放在点击位置正上方，水平居中；上面放不下就放到下面，靠近窗口边缘时往里收
    const above = shown.y - r.height - 14;
    const y = above < 8 ? shown.y + 18 : above;
    setPos(clampToViewport(shown.x - r.width / 2, y, r.width, r.height, innerWidth, innerHeight));
  }, [shown]);

  if (!shown) return null;
  return (
    <div
      key={shown.id}
      ref={ref}
      className={`copy-toast${leaving ? " leaving" : ""}${pos ? " placed" : ""}`}
      style={pos ? { left: pos.x, top: pos.y } : { left: 0, top: 0, visibility: "hidden" }}
      role="status"
      aria-live="polite"
    >
      <span className="copy-card ui-zoom">
      <span className="copy-logo" aria-hidden="true">
        <img src="/mooncli.svg" alt="" draggable={false} />
      </span>
      <span className="copy-text">
        <strong>已复制</strong>
        <span>{copySummary(shown.lines, shown.chars)}</span>
      </span>
      </span>
    </div>
  );
}

/** 页面底部居中的小提示，比如"缩放 110%"。 */
export function Notice() {
  const notice = useUi((s) => s.notice);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    if (!notice) return;
    setVisible(true);
    const t = setTimeout(() => setVisible(false), 1600);
    return () => clearTimeout(t);
  }, [notice]);
  if (!notice) return null;
  return (
    <div key={notice.id} className={`notice${visible ? "" : " hide"}`} role="status" aria-live="polite">
      <span className="notice-card ui-zoom">{notice.text}</span>
    </div>
  );
}
