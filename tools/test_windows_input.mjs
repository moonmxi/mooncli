// Exercise the packaged WebView2 app, the runner's Windows clipboard and its actual ConPTY.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { mkdtemp, readFile, readdir, rm, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

if (process.env.GITHUB_ACTIONS !== "true" || process.env.RUNNER_OS !== "Windows")
  throw new Error("Requires an isolated Windows CI runner");

async function until(fn, description) {
  let last;
  for (let i = 0; i < 160; i++) {
    try { const result = await fn(); if (result) return result; } catch (error) { last = error; }
    await sleep(250);
  }
  throw new Error("Timeout waiting for " + description, {cause:last});
}

const dir = await mkdtemp(join(tmpdir(), "mooncli-input-"));
const ready = join(dir, "ready");
const output = join(dir, "result");
const bundle = resolve("target/release/bundle/nsis");
const installer = (await readdir(bundle)).find(name => name.endsWith(".exe"));
assert.ok(installer, "NSIS installer");
const installDir = join(dir, "install");
execFileSync(join(bundle,installer), ["/S", "/D=" + installDir], {windowsHide:true, timeout:60000});
const exe = join(installDir,"mooncli.exe");
await access(exe);
const app = spawn(exe, [], { windowsHide:true, env:{
  ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:"--remote-debugging-port=19222",
}});
let ws;
try {
  const target = await until(async () => (await (await fetch("http://127.0.0.1:19222/json/list")).json())
    .find(t => t.type === "page" && t.url.includes("tauri.localhost")), "application WebView");
  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res,rej) => {ws.onopen = res; ws.onerror = rej;});
  let seq = 0;
  const pending = new Map();
  ws.onmessage = ({data}) => {
    const m = JSON.parse(data);
    if (pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  };
  const call = async (method,params) => {
    const id = ++seq;
    const reply = new Promise(resolve => pending.set(id,resolve));
    ws.send(JSON.stringify({id,method,params}));
    const msg = await reply;
    if (msg.error) throw Error(JSON.stringify(msg.error));
    return msg.result;
  };
  const evaluate = async expression => {
    const r = await call("Runtime.evaluate",{expression,returnByValue:true,awaitPromise:true});
    if (r.exceptionDetails) throw Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
    return r.result.value;
  };
  const enter = async () => {
    for (const type of ["keyDown","keyUp"]) await call("Input.dispatchKeyEvent",{
      type,key:"Enter",code:"Enter",windowsVirtualKeyCode:13, ...(type==="keyDown"?{text:"\r"}:{})
    });
  };
  await until(() => evaluate("!!document.querySelector('.xterm-helper-textarea') && !!document.querySelector('.sdot.s-prompt')"),"PowerShell prompt");
  await evaluate("document.querySelector('.xterm-helper-textarea').focus()");
  const psPath = p => "'" + p.replaceAll("'","''") + "'";
  const command = "[IO.File]::WriteAllText("+psPath(ready)+",'ready'); $items=@(1..3 | % { Read-Host }); [IO.File]::WriteAllLines("+psPath(output)+",[string[]]$items)";
  await call("Input.insertText",{text:command}); await enter();
  await until(async()=>{await access(ready);return true;},"PowerShell reader");
  execFileSync("pwsh",["-NoProfile","-Command","Set-Clipboard -Value '纯文本 clipboard'"],{windowsHide:true});
  for (const type of ["rawKeyDown","keyUp"]) await call("Input.dispatchKeyEvent",{
    type,modifiers:2,key:"v",code:"KeyV",windowsVirtualKeyCode:86,
  });
  await sleep(350); await enter();
  async function processInput() {
    const el=document.querySelector(".xterm-helper-textarea");
    el.dispatchEvent(new KeyboardEvent("keydown",{bubbles:true,key:"Process",keyCode:229}));
    el.value+="语音甲";
    el.dispatchEvent(new InputEvent("input",{bubbles:true,composed:true,inputType:"insertText",data:"语音甲"}));
  }
  async function composedInput() {
    const el=document.querySelector(".xterm-helper-textarea");
    el.dispatchEvent(new CompositionEvent("compositionstart",{bubbles:true}));
    el.value+="语音乙";
    el.dispatchEvent(new CompositionEvent("compositionend",{bubbles:true,data:"语音乙"}));
    await new Promise(r=>setTimeout(r,50));
    el.dispatchEvent(new InputEvent("input",{bubbles:true,composed:true,inputType:"insertText",data:"语音乙"}));
  }
  for (const fn of [processInput,composedInput]) {
    await evaluate("("+fn.toString()+")()");
    await sleep(120); await enter();
  }
  const result = await until(async()=> (await readFile(output,"utf8")).trimEnd(),"PTY result");
  assert.deepEqual(result.split(/\r?\n/),["纯文本 clipboard","语音甲","语音乙"]);
  console.log("WebView2 -> ConPTY: Windows clipboard and both IME commits received exactly once.");
} finally {
  ws?.close();
  if (app.exitCode === null) {
    const exited = new Promise(resolve => app.once("exit",resolve));
    app.kill();
    await exited;
  }
  await rm(dir,{recursive:true,force:true,maxRetries:5,retryDelay:200});
}
