"""检查 design/themes 下所有主题的颜色对比度（WCAG 2.x 公式）。

用法: python3 tools/check_contrast.py
不达标时打印问题并以非 0 退出，可直接接进 CI。
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
THEMES = ROOT / "design" / "themes"

ANSI = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
ANSI_ALL = ANSI + ["bright" + c.capitalize() for c in ANSI]


def lum(hex_color: str) -> float:
    h = hex_color.lstrip("#")
    r, g, b = (int(h[i : i + 2], 16) / 255 for i in (0, 2, 4))

    def ch(c):
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)


def ratio(a: str, b: str) -> float:
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def need_for(name: str, kind: str) -> float:
    # 浅色主题里 white / brightWhite 是“程序以为在深色背景上的白字”，
    # 会被映射成浅灰，只要求 3:1；暗色主题里 black 同理。
    # brightBlack 常用于注释、联想提示等次要文字，同样要求 3:1。
    soft = {"white", "brightWhite"} if kind == "light" else {"black", "brightBlack"}
    if name in soft or name == "brightBlack":
        return 3.0
    return 4.5


def check(path: pathlib.Path) -> list[str]:
    t = json.loads(path.read_text(encoding="utf-8"))
    term, ui, kind = t["terminal"], t["ui"], t["kind"]
    bg = term["background"]
    problems = []

    def want(label, a, b, need):
        r = ratio(a, b)
        mark = "ok " if r >= need else "LOW"
        print(f"  {mark} {label:<28} {r:5.2f}  (>= {need})")
        if r < need:
            problems.append(f"{path.name}: {label} {r:.2f} < {need}")

    print(f"\n{t['displayName']} ({path.name})")
    want("foreground / background", term["foreground"], bg, 7.0)
    want("selection text", term["foreground"], term["selectionBackground"], 4.5)
    want("cursor / background", term["cursor"], bg, 3.0)
    for name in ANSI_ALL:
        want(name, term[name], bg, need_for(name, kind))
    for surf in ("surface0", "surface1", "surface2"):
        want(f"ui.text / {surf}", ui["text"], ui[surf], 7.0)
        want(f"ui.textMuted / {surf}", ui["textMuted"], ui[surf], 4.5)
    want("ui.accent / surface1", ui["accent"], ui["surface1"], 3.0)
    want("ui.onAccent / accent", ui["onAccent"], ui["accent"], 4.5)
    return problems


def main() -> int:
    problems = []
    for p in sorted(THEMES.glob("*.json")):
        problems += check(p)
    if problems:
        print("\n不达标：")
        print("\n".join("  " + p for p in problems))
        return 1
    print("\n全部达标")
    return 0


if __name__ == "__main__":
    sys.exit(main())
