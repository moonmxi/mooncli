// 命令块（docs/02 §4、docs/05 §6）：靠 shell 集成的标记，知道每条命令从哪一行开始、输出在哪、成没成功。
//   - 提示符所在行的左侧画一个小圆点：运行中灰色、成功绿色、失败红色；
//   - 滚动条上同样位置画一道标记，长输出里也能一眼找到每条命令；
//   - Ctrl+↑ / Ctrl+↓ 在命令之间跳；可以一键选中某条命令的全部输出。
// 标记用 xterm 的 marker，行号随着回滚自动更新，被挤出回滚区的块自动作废。
import type { IDecoration, IMarker, Terminal } from "@xterm/xterm";

export interface Block {
  /** 提示符那一行 */
  prompt: IMarker;
  /** 命令开始执行时光标所在行（输出从这里开始）；还在提示符上时没有 */
  output?: IMarker;
  exitCode?: number | null;
  /** 正在运行 */
  running: boolean;
  deco?: IDecoration;
}

export interface BlockColors {
  running: string;
  ok: string;
  failed: string;
}

/** 某一行属于哪个块：最后一个提示符行不晚于它的块 */
export function blockAt<T extends { line: number }>(prompts: T[], line: number): number {
  let found = -1;
  for (let i = 0; i < prompts.length; i++) {
    if (prompts[i].line <= line) found = i;
    else break;
  }
  return found;
}

/** 从 from 行往上 / 往下找下一个提示符行，找不到返回 null */
export function nextPromptLine(lines: number[], from: number, dir: 1 | -1): number | null {
  if (dir < 0) {
    for (let i = lines.length - 1; i >= 0; i--) if (lines[i] < from) return lines[i];
    return null;
  }
  for (const l of lines) if (l > from) return l;
  return null;
}

const MAX_BLOCKS = 2000;

export class CommandBlocks {
  private blocks: Block[] = [];

  constructor(
    private term: Terminal,
    private colors: () => BlockColors,
    private enabled: () => boolean,
  ) {}

  private alive() {
    this.blocks = this.blocks.filter((b) => !b.prompt.isDisposed && b.prompt.line >= 0);
    if (this.blocks.length > MAX_BLOCKS) {
      for (const b of this.blocks.splice(0, this.blocks.length - MAX_BLOCKS)) {
        b.deco?.dispose();
        b.prompt.dispose();
      }
    }
    return this.blocks;
  }

  /** 收到提示符标记：在光标所在行开一个新块 */
  onPrompt() {
    if (this.term.buffer.active.type === "alternate") return;
    const last = this.alive().at(-1);
    // 同一行重复的提示符（比如按了 Ctrl+C 重画）不再开新块
    if (last && !last.output && last.prompt.line === this.term.buffer.active.baseY + this.term.buffer.active.cursorY) return;
    if (last && !last.output) this.drop(last);
    const prompt = this.term.registerMarker(0);
    if (!prompt) return;
    const b: Block = { prompt, running: false };
    this.blocks.push(b);
    prompt.onDispose(() => b.deco?.dispose());
  }

  onCommandStart() {
    const b = this.alive().at(-1);
    if (!b || b.output) return;
    b.output = this.term.registerMarker(0) ?? undefined;
    b.running = true;
    this.decorate(b);
  }

  onCommandEnd(exitCode: number | null) {
    const b = this.alive().at(-1);
    if (!b || !b.running) return;
    b.running = false;
    b.exitCode = exitCode;
    this.decorate(b);
  }

  private drop(b: Block) {
    b.deco?.dispose();
    b.prompt.dispose();
    this.blocks = this.blocks.filter((x) => x !== b);
  }

  private decorate(b: Block) {
    b.deco?.dispose();
    b.deco = undefined;
    if (!this.enabled()) return;
    const c = this.colors();
    const color = b.running ? c.running : b.exitCode === 0 || b.exitCode === undefined ? c.ok : c.failed;
    const deco = this.term.registerDecoration({
      marker: b.prompt,
      width: 1,
      overviewRulerOptions: { color, position: "left" },
    });
    if (!deco) return;
    deco.onRender((el) => {
      el.classList.add("cmd-mark");
      el.style.setProperty("--mark", color);
      el.title = b.running ? "运行中" : b.exitCode === 0 ? "成功" : `退出码 ${b.exitCode ?? "未知"}`;
    });
    b.deco = deco;
  }

  /** 主题或开关变了：重画全部标记 */
  refresh() {
    for (const b of this.alive()) if (b.output) this.decorate(b);
  }

  /** 跳到上一条 / 下一条命令（以视口顶端为参照） */
  jump(dir: 1 | -1): boolean {
    const lines = this.alive().map((b) => b.prompt.line);
    const top = this.term.buffer.active.viewportY;
    const target = nextPromptLine(lines, top, dir);
    if (target === null) {
      if (dir > 0) this.term.scrollToBottom();
      return false;
    }
    this.term.scrollToLine(target);
    return true;
  }

  /** 选中视口顶端那条命令的输出（不含提示符行）；返回是否选中了东西 */
  selectOutput(): boolean {
    const blocks = this.alive();
    const i = blockAt(blocks.map((b) => b.prompt), this.term.buffer.active.viewportY + 1);
    const b = blocks[i] ?? blocks.at(-1);
    if (!b?.output) return false;
    const start = b.output.line;
    const next = blocks[blocks.indexOf(b) + 1];
    const end = (next ? next.prompt.line : this.term.buffer.active.baseY + this.term.buffer.active.cursorY) - 1;
    if (end < start) return false;
    this.term.selectLines(start, end);
    return true;
  }

  dispose() {
    for (const b of this.blocks) {
      b.deco?.dispose();
      b.prompt.dispose();
      b.output?.dispose();
    }
    this.blocks = [];
  }
}
