// 内置背景图：按当前主题的颜色现画，所以换主题时背景色调跟着变（docs/08 §9）。
// 都是低对比、主体在角落、大面积留白的图，保证文字区干净。
import type { Theme } from "./theme";

export const BUILTIN_BACKGROUNDS = [
  { id: "builtin:mist", name: "晨雾远山" },
  { id: "builtin:moon", name: "月面" },
  { id: "builtin:waves", name: "浅水波" },
  { id: "builtin:paper", name: "纸纹" },
] as const;

export const isBuiltin = (id: string | null): id is string => !!id && id.startsWith("builtin:");

/** 固定种子的伪随机数，每次画出来一样 */
function rng(seed: number) {
  return () => {
    seed = (seed * 1664525 + 1013904223) % 4294967296;
    return seed / 4294967296;
  };
}

/**
 * 按给定尺寸画一张内置背景。构图按比例摆放（主体在右下角、文字区留白），
 * 线宽、波纹间距这些细节按面积缩放，所以小窗口和带鱼屏上看起来都一样。
 */
export function renderBuiltin(id: string, t: Theme, w = 1600, h = 1000): string {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const g = c.getContext("2d");
  if (!g) return "";
  const bg = t.terminal.background;
  const accent = t.ui.accent;
  const dark = t.kind === "dark";
  const grad = g.createLinearGradient(0, 0, 0, h);
  grad.addColorStop(0, bg);
  grad.addColorStop(1, t.ui.surface2);
  g.fillStyle = grad;
  g.fillRect(0, 0, w, h);
  const r = rng(7);
  // 以 1600×1000 为基准的缩放系数
  const k = Math.sqrt((w * h) / (1600 * 1000));

  if (id === "builtin:mist") {
    g.globalAlpha = dark ? 0.2 : 0.55;
    g.fillStyle = dark ? t.ui.text : "#FFFFFF";
    g.beginPath();
    g.arc(w * 0.8, h * 0.24, 70 * k, 0, Math.PI * 2);
    g.fill();
    [
      [0.66, 0.1],
      [0.75, 0.14],
      [0.86, 0.2],
    ].forEach(([base, a], i) => {
      g.globalAlpha = a;
      g.fillStyle = accent;
      g.beginPath();
      g.moveTo(w * 0.3, h);
      for (let x = w * 0.3; x <= w; x += Math.max(3, 6 * k)) {
        const p = (x - w * 0.3) / (w * 0.7);
        g.lineTo(x, h * base - (Math.sin(p * (3 + i) + i) * 40 + Math.sin(p * 9 + i * 2) * 12 + p * 70) * (h / 1000));
      }
      g.lineTo(w, h);
      g.closePath();
      g.fill();
    });
  } else if (id === "builtin:moon") {
    const cx = w * 0.86;
    const cy = h * 0.86;
    const R = Math.min(h * 0.55, w * 0.42);
    g.globalAlpha = dark ? 0.16 : 0.22;
    g.fillStyle = dark ? t.ui.textMuted : t.ui.surface2;
    g.beginPath();
    g.arc(cx, cy, R, 0, Math.PI * 2);
    g.fill();
    // 环形山
    for (let i = 0; i < 26; i++) {
      const a = r() * Math.PI * 2;
      const d = r() * R * 0.9;
      const cr = (8 + r() * 46) * (R / 550);
      g.globalAlpha = 0.05 + r() * 0.07;
      g.strokeStyle = dark ? t.ui.text : t.ui.textMuted;
      g.lineWidth = Math.max(1, 2 * k);
      g.beginPath();
      g.arc(cx + Math.cos(a) * d, cy + Math.sin(a) * d, cr, 0, Math.PI * 2);
      g.stroke();
    }
  } else if (id === "builtin:waves") {
    g.strokeStyle = accent;
    g.lineWidth = Math.max(1, 1.4 * k);
    for (let i = 0; i < 14; i++) {
      g.globalAlpha = 0.04 + i * 0.008;
      const y0 = h * 0.62 + i * 26 * (h / 1000);
      g.beginPath();
      for (let x = 0; x <= w; x += Math.max(4, 8 * k)) {
        const y = y0 + (Math.sin(x / (120 * k) + i * 0.7) * (6 + i) + Math.sin(x / (47 * k) + i) * 2) * k;
        if (x === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      }
      g.stroke();
    }
  } else if (id === "builtin:paper") {
    // 细小纤维
    g.strokeStyle = dark ? t.ui.text : t.ui.textMuted;
    for (let i = 0, n = Math.round(1400 * k * k); i < n; i++) {
      g.globalAlpha = 0.025 + r() * 0.03;
      const x = r() * w;
      const y = r() * h;
      const len = (4 + r() * 16) * k;
      const a = r() * Math.PI;
      g.lineWidth = (0.6 + r() * 0.6) * Math.max(1, k);
      g.beginPath();
      g.moveTo(x, y);
      g.lineTo(x + Math.cos(a) * len, y + Math.sin(a) * len);
      g.stroke();
    }
  }
  g.globalAlpha = 1;
  return c.toDataURL("image/png");
}

/** CSS 里的 background-size / repeat */
export function fitCss(fit: string): { size: string; repeat: string } {
  switch (fit) {
    case "contain":
      return { size: "contain", repeat: "no-repeat" };
    case "tile":
      return { size: "auto", repeat: "repeat" };
    case "center":
      return { size: "auto", repeat: "no-repeat" };
    default:
      return { size: "cover", repeat: "no-repeat" };
  }
}
