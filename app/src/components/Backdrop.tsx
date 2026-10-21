// 终端区的背景层：底色 → 图片（透明度、模糊、亮度、饱和度）→ 和底色同色的蒙版 → 终端文字（docs/03 §3）。
import { useEffect, useState } from "react";
import type { Backend } from "../backend/types";
import type { AppSettings } from "../state/settings";
import { fitCss, isBuiltin, renderBuiltin } from "../theme/backgrounds";
import type { Theme } from "../theme/theme";

/** 读出背景图的地址：内置的现画，导入的找后端要。 */
export function useBackgroundUrl(image: string | null, theme: Theme, backend: Backend): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    if (!image) setUrl(null);
    else if (isBuiltin(image)) setUrl(renderBuiltin(image, theme));
    else
      backend
        .backgroundLoad(image)
        .then((u) => alive && setUrl(u))
        .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [image, theme, backend]);
  return url;
}

export function Backdrop({ bg, url, theme }: { bg: AppSettings["background"]; url: string | null; theme: Theme }) {
  if (!url) return null;
  const fit = fitCss(bg.fit);
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
