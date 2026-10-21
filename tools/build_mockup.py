"""把 design/themes/*.json 嵌进原型页面，生成 design/mockup/index.html。

用法: python3 tools/build_mockup.py   （改了主题或 index.src.html 后重新运行）
"""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
src = (ROOT / "design/mockup/index.src.html").read_text(encoding="utf-8")
themes = {
    p.stem: json.loads(p.read_text(encoding="utf-8"))
    for p in sorted((ROOT / "design/themes").glob("*.json"))
}
out = src.replace("/*THEMES*/null", json.dumps(themes, ensure_ascii=False))
(ROOT / "design/mockup/index.html").write_text(out, encoding="utf-8")
print("已生成 design/mockup/index.html")
