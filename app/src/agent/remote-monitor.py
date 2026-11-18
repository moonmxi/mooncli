"""Report subscription windows through the current interactive Unix terminal."""
import json
import os
import re
import shutil
import signal
import subprocess
import time


def claude():
    output = subprocess.check_output(
        ["claude", "-p", "/usage", "--no-session-persistence", "--output-format", "json"],
        stdin=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=25, text=True,
    )
    limits = {}
    for label, key in [("Current session", "fiveHour"), ("Current week (all models)", "sevenDay")]:
        match = re.search(re.escape(label) + r":\s*([\d.]+)%\s+used", json.loads(output)["result"], re.I)
        if match:
            limits[key] = {"usedPercent": float(match[1])}
    return limits


def codex():
    proc = subprocess.Popen(["codex", "app-server"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, text=True)
    def send(message):
        proc.stdin.write(json.dumps(message) + "\n")
        proc.stdin.flush()

    def result(request_id):
        for line in proc.stdout:
            msg = json.loads(line)
            if msg.get("id") == request_id:
                return msg["result"]
        raise EOFError()

    try:
        signal.alarm(25)
        send({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "mooncli", "version": "1"}}})
        result(1)
        send({"method": "initialized"})
        send({"id": 2, "method": "account/rateLimits/read"})
        data = result(2)
        snapshot = (data.get("rateLimitsByLimitId") or {}).get("codex") or data["rateLimits"]
        limits = {}
        for field, default in [("primary", 300), ("secondary", 10080)]:
            w = snapshot.get(field)
            if w and w.get("usedPercent") is not None:
                key = "sevenDay" if (w.get("windowDurationMins") or default) > 300 else "fiveHour"
                limits[key] = {"usedPercent": w["usedPercent"], "resetsAt": w.get("resetsAt")}
        return limits
    finally:
        signal.alarm(0)
        proc.kill()
        proc.wait()


def timeout(*_):
    raise TimeoutError()


def main():
    parent = os.getppid()
    signal.signal(signal.SIGALRM, timeout)
    with open("/dev/tty", "w", encoding="ascii") as tty:
        while os.getppid() == parent and parent > 1:
            for name, query in [("claude", claude), ("codex", codex)]:
                if not shutil.which(name):
                    continue
                try:
                    windows = query()
                except (OSError, ValueError, KeyError, EOFError, subprocess.SubprocessError):
                    continue
                if windows:
                    payload = json.dumps({"kind": name, **windows}, allow_nan=False)
                    tty.write("\x1b]6974;" + payload + "\x07")
                    tty.flush()
            for _ in range(300):
                if os.getppid() != parent:
                    return
                time.sleep(1)


if __name__ == "__main__":
    main()
