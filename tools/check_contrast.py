"""检查 design/themes 下所有主题的颜色对比度。规则在 resources/contrast-rules.json。

用法: python3 tools/check_contrast.py
不达标时打印问题并以非 0 退出，CI 会跑。不达标时可以先运行 tools/tune_palette.py 自动微调。
"""
import json
import sys

from contrast_rules import ANSI_ALL, ROOT, RULES, ratio, range_for

# Windows 上 Python 默认用系统代码页输出，打印中文会报错
sys.stdout.reconfigure(encoding="utf-8")

THEMES = ROOT / "design" / "themes"


def check(path) -> list[str]:
    t = json.loads(path.read_text(encoding="utf-8"))
    term, ui = t["terminal"], t["ui"]
    rules = RULES[t["kind"]]
    bg = term["background"]
    problems = []

    def want(label, value, lo, hi=None):
        ok = value >= lo and (hi is None or value <= hi)
        rng = f">= {lo}" if hi is None else f"{lo} ~ {hi}"
        print(f"  {'ok ' if ok else 'BAD'} {label:<34} {value:5.2f}  ({rng})")
        if not ok:
            problems.append(f"{path.name}: {label} {value:.2f} 不在 {rng}")

    print(f"\n{t['displayName']} ({path.name})")
    want("foreground", ratio(term["foreground"], bg), rules["foreground"][0])
    want("selection 上的文字", ratio(term["foreground"], term["selectionBackground"]), rules["selection"][0])
    want("cursor", ratio(term["cursor"], bg), rules["cursor"][0])
    for name in ANSI_ALL:
        lo, hi, why = range_for(t, name)
        want(f"{name} [{why}]", ratio(term[name], bg), lo, hi)
    u = RULES["ui"]
    for surf in ("surface0", "surface1", "surface2"):
        want(f"ui.text / {surf}", ratio(ui["text"], ui[surf]), u["text"])
        want(f"ui.textMuted / {surf}", ratio(ui["textMuted"], ui[surf]), u["textMuted"])
    want("ui.accent / surface1", ratio(ui["accent"], ui["surface1"]), u["accentOnSurface1"])
    want("ui.onAccent / accent", ratio(ui["onAccent"], ui["accent"]), u["onAccent"])
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
