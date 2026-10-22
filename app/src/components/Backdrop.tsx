// 终端区的背景层：底色 → 图片（透明度、模糊、亮度、饱和度）→ 和底色同色的蒙版 → 终端文字（docs/03 §3）。
import { useEffect, useState, type RefObject } from "react";
import type { Backend } from "../backend/types";
import type { AppSettings } from "../state/settings";
import { fitCss, isBuiltin, renderBuiltin } from "../theme/backgrounds";
import type { Theme } from "../theme/theme";

/** 量出元素的像素尺寸（按设备像素、对齐到 32px），拖动窗口时停下来 150ms 才更新，免得反复重画。 */
export function useRenderSize(ref: RefObject<HTMLElement | null>): { w: number; h: number } | null {
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let timer = 0;
    const measure = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const q = (v: number) => Math.max(64, Math.min(3840, Math.ceil((v * dpr) / 32) * 32));
      const w = q(el.clientWidth);
      const h = q(el.clientHeight);
      setSize((s) => (s && s.w === w && s.h === h ? s : { w, h }));
    };
    measure();
    const ro = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = window.setTimeout(measure, 150);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      clearTimeout(timer);
    };
  }, [ref]);
  return size;
}

/**
 * 读出背景图的地址：内置的按终端区的实际尺寸现画（窗口变了就重画，所以总是正好铺满、构图不被裁掉），
 * 导入的找后端要，再由 CSS 按"填满"等方式缩放。
 */
export function useBackgroundUrl(
  image: string | null,
  theme: Theme,
  backend: Backend,
  size: { w: number; h: number } | null,
): string | null {
  const [url, setUrl] = useState<string | null>(null);
  const builtin = isBuiltin(image);
  const w = builtin ? size?.w : undefined;
  const h = builtin ? size?.h : undefined;
  useEffect(() => {
    let alive = true;
    if (!image) setUrl(null);
    else if (isBuiltin(image)) {
      if (w && h) setUrl(renderBuiltin(image, theme, w, h));
    } else
      backend
        .backgroundLoad(image)
        .then((u) => alive && setUrl(u))
        .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [image, theme, backend, w, h]);
  return url;
}

export function Backdrop({ bg, url, theme }: { bg: AppSettings["background"]; url: string | null; theme: Theme }) {
  if (!url) return null;
  // 内置图本来就按终端区尺寸画的，直接拉满；导入的图按用户选的方式
  const fit = isBuiltin(bg.image) ? { size: "100% 100%", repeat: "no-repeat" } : fitCss(bg.fit);
  return (
    <div className="backdrop" aria-hidden="true">
      <div
        className="backdrop-image"
        style={{
          backgroundImage: `url("${url}")`,
          backgroundSize: fit.size,
          backgroundRepeat: fit.repeat,
          backgroundPosition: bg.position,
          opacity: bg.opacity,
          filter: `blur(${bg.blur}px) brightness(${bg.brightness}) saturate(${bg.saturation})`,
          // 模糊会让边缘变透明，放大一点盖住
          inset: bg.blur > 0 ? -bg.blur * 2 : 0,
        }}
      />
      <div className="backdrop-veil" style={{ background: theme.terminal.background, opacity: bg.overlay }} />
    </div>
  );
}
