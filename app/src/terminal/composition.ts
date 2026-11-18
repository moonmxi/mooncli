/** xterm 在 compositionend 后的下一轮从 textarea 取提交文本。同一轮的 insertText 由这条路径统一发送。 */
export function guardComposition(host: HTMLElement): () => void {
  let composing = false;
  let commit: string | null = null;
  let timer: ReturnType<typeof setTimeout>;
  const start = () => { composing = true; commit = null; clearTimeout(timer); };
  const end = (event: CompositionEvent) => {
    composing = false;
    commit = event.data;
    clearTimeout(timer);
    timer = setTimeout(() => { commit = null; }, 0);
  };
  const input = (event: Event) => {
    const e = event as InputEvent;
    if (e.inputType === "insertText" && (composing || (commit !== null && e.data === commit))) {
      e.stopPropagation();
    }
  };
  host.addEventListener("compositionstart", start, true);
  host.addEventListener("compositionend", end, true);
  host.addEventListener("input", input, true);
  return () => {
    clearTimeout(timer);
    host.removeEventListener("compositionstart", start, true);
    host.removeEventListener("compositionend", end, true);
    host.removeEventListener("input", input, true);
  };
}
