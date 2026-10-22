"""检查仓库里内嵌的 PowerShell 脚本有没有语法错误（用 pwsh 自己的解析器）。

检查的对象：
- .github/workflows/ci.yml 里 shell: pwsh 的步骤（发布步骤出错只有在发版本时才会暴露）
- crates/mooncli-pty/src/shell.rs 里安装 pwsh 7 的脚本 INSTALL_PWSH_SCRIPT

特别注意：PowerShell 把中文的弯引号 “ ” ‘ ’ 也当成字符串的引号，写在字符串里会让脚本解析失败。
用法：python tools/check_ps_scripts.py（需要 pwsh）
"""

import json
import re
import subprocess
import sys
from pathlib import Path

import yaml

sys.stdout.reconfigure(encoding="utf-8")

ROOT = Path(__file__).resolve().parent.parent


def scripts():
    wf = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"))
    for job in wf["jobs"].values():
        for st in job.get("steps", []):
            if st.get("shell") == "pwsh" and "run" in st:
                # GitHub 的 ${{ }} 表达式先换成占位文字
                yield f"ci.yml：{st.get('name', '')}", re.sub(r"\$\{\{[^}]*\}\}", "X", st["run"])
    rs = (ROOT / "crates/mooncli-pty/src/shell.rs").read_text(encoding="utf-8")
    m = re.search(r'INSTALL_PWSH_SCRIPT: &str = r#"(.*?)"#;', rs, re.S)
    if not m:
        sys.exit("shell.rs 里找不到 INSTALL_PWSH_SCRIPT")
    yield "shell.rs：INSTALL_PWSH_SCRIPT", m.group(1)


PARSE = r"""
$items = [Console]::In.ReadToEnd() | ConvertFrom-Json
$bad = 0
foreach ($i in $items) {
  $errs = $null
  [void][System.Management.Automation.Language.Parser]::ParseInput($i.script, [ref]$null, [ref]$errs)
  if ($errs.Count) { $bad++; Write-Output ("✗ {0}：第 {1} 行 {2}" -f $i.name, $errs[0].Extent.StartLineNumber, $errs[0].Message) }
  else { Write-Output ("✓ {0}" -f $i.name) }
}
exit $bad
"""

items = [{"name": n, "script": s} for n, s in scripts()]
r = subprocess.run(
    ["pwsh", "-NoProfile", "-NonInteractive", "-Command", PARSE],
    input=json.dumps(items, ensure_ascii=False),
    text=True,
    encoding="utf-8",
    capture_output=True,
)
print(r.stdout, end="")
if r.stderr:
    print(r.stderr, file=sys.stderr)
sys.exit(1 if r.returncode else 0)
