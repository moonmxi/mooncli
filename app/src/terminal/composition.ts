/** WebView2 may deliver the same IME commit through composition, paste and input.
 * Keep xterm's preedit rendering; match a second delivery only within one edit gesture. */
export function guardComposition(host: HTMLElement, insert: (text: string) => void) {
  let composing = false;
  let processKey = false;
  let skipComposedOutput: string | undefined;
  let pending: { text: string; source: "composition" | "input" | "paste" } | undefined;
  let expiry: ReturnType<typeof setTimeout>;
  const reset = () => {
    clearTimeout(expiry);
    pending = undefined;
    skipComposedOutput = undefined;
    processKey = false;
  };
  const remember = (text: string, source: NonNullable<typeof pending>["source"]) => {
    clearTimeout(expiry);
    pending = { text, source };
    expiry = setTimeout(reset, 750);
  };
  const key = (e: KeyboardEvent) => {
    if (e.keyCode === 229 && !composing) {
      // Otherwise xterm schedules a second textarea diff in addition to the native input event.
      processKey = true;
      e.stopPropagation();
    } else if (!composing && !["Control", "Alt", "Shift", "Meta"].includes(e.key)) reset();
  };
  const start = () => {
    if (pending?.source !== "paste") reset();
    processKey = false;
    composing = true;
  };
  const end = (e: CompositionEvent) => {
    composing = false;
    if (e.data) {
      if (pending?.source === "paste" && pending.text === e.data) skipComposedOutput = e.data;
      remember(e.data, "composition");
    }
  };
  const isCommit = (e: InputEvent) =>
    e.inputType === "insertText" || e.inputType === "insertReplacementText" || e.inputType === "insertFromComposition";
  const echo = (e: InputEvent) => !!e.data && pending?.text === e.data && pending.source !== "input";
  const before = (event: Event) => {
    const e = event as InputEvent;
    if (!composing && isCommit(e) && echo(e) && e.cancelable) {
      // Prevent the duplicate from being appended to the textarea read by xterm's deferred commit.
      if (pending?.source === "composition" && e.target instanceof HTMLTextAreaElement &&
          !e.target.value.endsWith(e.data!)) return;
      e.preventDefault();
      e.stopPropagation();
      pending = undefined;
    }
  };
  const input = (event: Event) => {
    const e = event as InputEvent;
    if (!isCommit(e) || !e.data) return;
    if (composing || echo(e)) {
      e.stopPropagation();
      if (!composing) pending = undefined;
      return;
    }
    if (processKey || e.inputType === "insertReplacementText") {
      e.stopPropagation();
      insert(e.data);
      processKey = false;
    }
    remember(e.data, "input");
  };
  const acceptPaste = (text: string) => {
    if (pending && pending.source !== "paste" && pending.text === text) { reset(); return false; }
    remember(text, "paste");
    return true;
  };
  host.addEventListener("keydown", key, true);
  host.addEventListener("compositionstart", start, true);
  host.addEventListener("compositionend", end, true);
  host.addEventListener("beforeinput", before, true);
  host.addEventListener("input", input, true);
  host.addEventListener("blur", reset, true);
  const acceptTerminal = (text: string) => {
    if (skipComposedOutput !== text) return true;
    skipComposedOutput = undefined;
    return false;
  };
  return { acceptPaste, acceptTerminal, dispose() {
    reset();
    host.removeEventListener("keydown", key, true);
    host.removeEventListener("compositionstart", start, true);
    host.removeEventListener("compositionend", end, true);
    host.removeEventListener("beforeinput", before, true);
    host.removeEventListener("input", input, true);
    host.removeEventListener("blur", reset, true);
  } };
}
