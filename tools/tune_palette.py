"""按 resources/contrast-rules.json 自动微调主题的 16 色，然后写回 design/themes/*.json。

做法：
1. 和主题强调色色相太近的颜色（比如霜蓝主题里的蓝色），先把色相往外转到规定的角度，
   并按"接近主题色"的更高要求处理，这样它们在同色系背景上也跳得出来；
2. 只调明度、保留饱和度，把对比度推进规定范围（目标是下限再高一点，不会改得过头）。

用法: python3 tools/tune_palette.py && python3 tools/check_contrast.py
"""
import colorsys
import json
import sys

from contrast_rules import ANSI_ALL, CHROMATIC, ROOT, RULES, base_name, hexs, hue, hue_dist, ratio, range_for, rgb

# Windows 上 Python 默认用系统代码页输出，打印中文会报错
sys.stdout.reconfigure(encoding="utf-8")


def rotate_away(color: str, accent_h: float, degrees: float, others: list[float]) -> str:
    """把色相转到离强调色至少 degrees 度。两个方向都试，选离其它颜色（比如蓝色旁边的青色）更远的那边。"""
    h, l, s = colorsys.rgb_to_hls(*rgb(color))
    cur = h * 360
    if hue_dist(cur, accent_h) >= degrees:
        return color
    best = None
    for sign in (1, -1):
        new = cur
        while hue_dist(new, accent_h) < degrees + 2:
            new = (new + sign) % 360
        room = min((hue_dist(new, o) for o in others), default=180)
        if best is None or room > best[0]:
            best = (room, new)
    return hexs(colorsys.hls_to_rgb(best[1] / 360, l, s))


def fit_lightness(color: str, bg: str, lo: float, hi) -> str:
    h, l, s = colorsys.rgb_to_hls(*rgb(color))
    light_bg = ratio("#FFFFFF", bg) < ratio("#000000", bg)
    target_lo = lo + 0.12
    target_hi = None if hi is None else hi - 0.12
    for _ in range(400):
        c = hexs(colorsys.hls_to_rgb(h, l, s))
        r = ratio(c, bg)
        if r < target_lo:
            l += -0.004 if light_bg else 0.004
        elif target_hi is not None and r > target_hi:
            l += 0.004 if light_bg else -0.004
        else:
            return c
        l = min(1, max(0, l))
    return hexs(colorsys.hls_to_rgb(h, l, s))


def tune(t: dict) -> list[str]:
    term = t["terminal"]
    bg = term["background"]
    accent_h = hue(t["ui"]["accent"])
    near = RULES[t["kind"]]["nearAccent"]
    changes = []
    for name in ANSI_ALL:
        old = term[name]
        lo, hi, _ = range_for(t, name)  # 按原色相判断要求，转开色相后仍用更高的要求
        c = old
        if base_name(name) in CHROMATIC and near.get("rotate"):
            others = [hue(term[o]) for o in CHROMATIC if o != base_name(name)]
            c = rotate_away(c, accent_h, near["degrees"], others)
        c = fit_lightness(c, bg, lo, hi)
        if c != old:
            term[name] = c
            changes.append(f"{name:<14} {old} → {c}  ({ratio(c, bg):.2f})")
    return changes


def main():
    for p in sorted((ROOT / "design" / "themes").glob("*.json")):
        t = json.loads(p.read_text(encoding="utf-8"))
        changes = tune(t)
        print(f"\n{t['displayName']}：{len(changes)} 处")
        for c in changes:
            print("  " + c)
        p.write_text(json.dumps(t, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
