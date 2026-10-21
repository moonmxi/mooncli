"""下载微软官方的新版 ConPTY（conpty.dll + OpenConsole.exe），放进安装包资源目录。

为什么要附带：较老的 Windows 自带的 ConPTY 会把它不认识的终端标记（OSC 133、OSC 7 等）吞掉，
shell 集成就收不到任何信号，状态栏一直显示"未集成"。Windows Terminal、WezTerm、VS Code 都附带新版来解决这个问题。

用法: python3 tools/fetch_conpty.py   （CI 打包前运行；本地开发可以不运行，会退回系统自带的 ConPTY）
来源: https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY  （MIT 协议，microsoft/terminal 项目）
"""
import hashlib
import io
import pathlib
import sys
import urllib.request
import zipfile

VERSION = "1.25.260930003"
SHA256 = "02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee"
ARCH = "x64"

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "app" / "src-tauri" / "resources" / "conpty"
URL = (
    "https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/"
    f"{VERSION}/microsoft.windows.console.conpty.{VERSION}.nupkg"
)
FILES = {
    f"runtimes/win-{ARCH}/native/conpty.dll": "conpty.dll",
    f"build/native/runtimes/{ARCH}/OpenConsole.exe": "OpenConsole.exe",
}


def main() -> int:
    print(f"下载 ConPTY {VERSION} …")
    data = urllib.request.urlopen(URL, timeout=120).read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != SHA256:
        print(f"校验失败：期望 {SHA256}，实际 {digest}", file=sys.stderr)
        return 1
    OUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        for src, dst in FILES.items():
            (OUT / dst).write_bytes(z.read(src))
            print(f"  {dst}  {len(z.read(src)) // 1024} KB")
    (OUT / "VERSION.txt").write_text(f"Microsoft.Windows.Console.ConPTY {VERSION}（MIT）\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
