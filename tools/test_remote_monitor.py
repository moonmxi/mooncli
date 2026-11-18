"""Exercise the shipped remote collector on a Unix PTY using fixture CLI processes."""
import json
import os
from pathlib import Path
import pty
import select
import signal
import sys
import tempfile
import time


def main():
    monitor = Path(__file__).resolve().parents[1] / "app/src/agent/remote-monitor.py"
    with tempfile.TemporaryDirectory() as temp:
        fixtures = {
            "claude": """import json
print(json.dumps({"result": "Current session: 12% used"}))
""",
            "codex": """import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if msg["method"] == "initialize":
        print(json.dumps({"id": 1, "result": {}}), flush=True)
    if msg["method"] == "account/rateLimits/read":
        print(json.dumps({"id": 2, "result": {"rateLimits": {"primary": {"usedPercent": 17}}}}), flush=True)
""",
        }
        for name, body in fixtures.items():
            path = Path(temp, name)
            path.write_text("#!" + sys.executable + "\n" + body)
            path.chmod(0o700)
        pid, fd = pty.fork()
        if pid == 0:
            os.environ["PATH"] = temp
            os.execv(sys.executable, [sys.executable, str(monitor)])
        try:
            data = b""
            deadline = time.monotonic() + 10
            while data.count(b"\x07") < 2:
                if not select.select([fd], [], [], max(0, deadline - time.monotonic()))[0]:
                    raise AssertionError("Timed out waiting for PTY quota frames: " + repr(data))
                data += os.read(fd, 2048)
            frames = [json.loads(s.removeprefix(b"\x1b]6974;")) for s in data.split(b"\x07") if s]
            assert [(f["kind"], f["fiveHour"]["usedPercent"]) for f in frames] == [("claude", 12), ("codex", 17)]
        finally:
            os.kill(pid, signal.SIGTERM)
            os.waitpid(pid, 0)
            os.close(fd)


if __name__ == "__main__":
    main()
