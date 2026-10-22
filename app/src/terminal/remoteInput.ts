// 从屏幕上读出用户在远程 shell 里输入的命令（docs/06 §4.1）。
//
// 远程 shell 没有 mooncli 的集成脚本，不会把命令文字告诉我们。但终端里本来就显示着：
// 远程回到提示符时开始留意，用户按下第一个键的那一刻光标所在的位置就是输入的起点；
// 命令开始执行时，从起点读到光标处就是这条命令。远程装了 shell 集成（发 133;B）时，起点更准。
import type { IBuffer, Terminal } from "@xterm/xterm";

export interface Cell {
  /** 在整个缓冲区里的行号（含回滚区） */
  row: number;
  col: number;
}

export function cursorCell(buf: IBuffer): Cell {
  return { row: buf.baseY + buf.cursorY, col: buf.cursorX };
}

/** 从 from 读到 to（不含 to 所在的格子）。自动换行的行接起来，真正的换行保留。 */
export function readBetween(buf: IBuffer, from: Cell, to: Cell): string {
  // 回车换行之后光标在下一行行首，那一行还不属于命令
  const last = to.col === 0 && to.row > from.row ? to.row - 1 : to.row;
  let text = "";
  for (let r = from.row; r <= last; r++) {
    const line = buf.getLine(r);
    if (!line) break;
    const start = r === from.row ? from.col : 0;
    const end = r === to.row ? to.col : undefined;
    text += line.translateToString(true, start, end);
    const next = buf.getLine(r + 1);
    if (r < last && !next?.isWrapped) text += "\n";
  }
  return text.trim();
}

/** 这些键会让远程 shell 重画整行或清屏（Ctrl+C、Ctrl+L、Ctrl+U），按了以后重新找起点 */
const RESETS = new Set(["\x03", "\x0c", "\x15", "\x04"]);

export class RemoteInput {
  private start: Cell | null = null;
  private armed = false;

  constructor(private term: Terminal) {}

  /** 远程回到了提示符。exact：远程 shell 用标记告诉了我们提示符在哪结束，光标此刻就是起点 */
  arm(exact = false) {
    this.armed = true;
    this.start = exact ? cursorCell(this.term.buffer.active) : null;
  }

  disarm() {
    this.armed = false;
    this.start = null;
  }

  /** 用户按键，在发给远程之前调用：这时远程还没回显，光标就在输入起点 */
  onInput(data: string) {
    if (!this.armed) return;
    if (RESETS.has(data)) {
      this.start = null;
      return;
    }
    this.start ??= cursorCell(this.term.buffer.active);
  }

  /** 远程开始执行命令：读出文字。没有起点（没按过键、直接回车）返回空字符串 */
  take(): string {
    const start = this.start;
    this.disarm();
    if (!start) return "";
    const buf = this.term.buffer.active;
    const end = cursorCell(buf);
    if (end.row < start.row) return "";
    return readBetween(buf, start, end);
  }
}
