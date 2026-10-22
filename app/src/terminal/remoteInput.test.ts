import { Terminal } from "@xterm/headless";
import { describe, expect, it } from "vitest";
import { RemoteInput } from "./remoteInput";

const write = (t: Terminal, s: string) => new Promise<void>((r) => t.write(s, r));

function setup(cols = 40) {
  const term = new Terminal({ cols, rows: 10, allowProposedApi: true });
  // 读写只用到 buffer，headless 版本的接口和界面里的一样
  const input = new RemoteInput(term as unknown as ConstructorParameters<typeof RemoteInput>[0]);
  return { term, input };
}

describe("从屏幕读出远程命令", () => {
  it("从第一次按键的位置读到回车", async () => {
    const { term, input } = setup();
    await write(term, "Welcome\r\nme@box:~$ ");
    input.arm();
    input.onInput("l"); // 按键时远程还没回显
    await write(term, "ls -la\r\n");
    expect(input.take()).toBe("ls -la");
  });

  it("长命令自动换行后接起来", async () => {
    const { term, input } = setup(20);
    await write(term, "box$ ");
    input.arm();
    input.onInput("e");
    await write(term, "echo 0123456789abcdefghij\r\n");
    expect(input.take()).toBe("echo 0123456789abcdefghij");
  });

  it("远程用标记给出提示符结尾时直接用光标位置", async () => {
    const { term, input } = setup();
    await write(term, "[root@c7 ~]# ");
    input.arm(true);
    await write(term, "uptime\r\n");
    expect(input.take()).toBe("uptime");
  });

  it("没按键直接回车、或按了 Ctrl+C 之后没再输入，读出空字符串", async () => {
    const { term, input } = setup();
    await write(term, "box$ ");
    input.arm();
    await write(term, "\r\n");
    expect(input.take()).toBe("");
    input.arm();
    input.onInput("\x03");
    expect(input.take()).toBe("");
  });

  it("Ctrl+L 清屏后从新的提示符读", async () => {
    const { term, input } = setup();
    await write(term, "a\r\nb\r\nbox$ ");
    input.arm();
    input.onInput("\x0c");
    await write(term, "\x1b[H\x1b[2Jbox$ ");
    input.onInput("p");
    await write(term, "pwd\r\n");
    expect(input.take()).toBe("pwd");
  });
});
