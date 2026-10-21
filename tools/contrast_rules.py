"""读 resources/contrast-rules.json，给每个 ANSI 色算出它该满足的对比度范围。"""
import colorsys
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
RULES = json.loads((ROOT / "resources" / "contrast-rules.json").read_text(encoding="utf-8"))

ANSI = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
ANSI_ALL = ANSI + ["bright" + c.capitalize() for c in ANSI]
CHROMATIC = {"red", "green", "yellow", "blue", "magenta", "cyan"}


def rgb(h: str):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) / 255 for i in (0, 2, 4))


def hexs(c) -> str:
    return "#%02X%02X%02X" % tuple(max(0, min(255, round(x * 255))) for x in c)


def lum(h: str) -> float:
    def ch(c):
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = rgb(h)
    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)


def ratio(a: str, b: str) -> float:
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def hue(h: str) -> float:
    return colorsys.rgb_to_hls(*rgb(h))[0] * 360


def hue_dist(a: float, b: float) -> float:
    d = abs(a - b) % 360
    return min(d, 360 - d)


def base_name(name: str) -> str:
    return name[6:].lower() if name.startswith("bright") else name


def range_for(theme: dict, name: str):
    """返回 (最小, 最大, 原因)"""
    r = RULES[theme["kind"]]
    ansi = r["ansi"]
    if name in ansi:
        lo, hi = ansi[name]
        return lo, hi, name
    bright = name.startswith("bright")
    lo, hi = ansi["bright" if bright else "normal"]
    why = "bright" if bright else "normal"
    if base_name(name) in CHROMATIC:
        near = r["nearAccent"]
        if hue_dist(hue(theme["terminal"][name]), hue(theme["ui"]["accent"])) < near["degrees"]:
            lo = max(lo, near["bright" if bright else "normal"])
            why += "+接近主题色"
    return lo, hi, why
