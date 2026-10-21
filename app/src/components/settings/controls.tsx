// 设置页用的小控件：开关、滑杆、分段按钮、下拉、文本框，以及一行设置的布局。
import { useState, type ReactNode } from "react";

export function Row({ label, desc, children, wide }: { label: string; desc?: ReactNode; children: ReactNode; wide?: boolean }) {
  return (
    <div className={`set-row${wide ? " wide" : ""}`}>
      <div className="set-text">
        <div className="set-label">{label}</div>
        {desc && <div className="set-desc">{desc}</div>}
      </div>
      <div className="set-control">{children}</div>
    </div>
  );
}

export function Group({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="set-group">
      <header className="set-group-head">
        <h3>{title}</h3>
        {action}
      </header>
      <div className="set-group-body">{children}</div>
    </section>
  );
}

export function Toggle({ value, onChange, label }: { value: boolean; onChange(v: boolean): void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={value}
      aria-label={label}
      className={`toggle${value ? " on" : ""}`}
      onClick={() => onChange(!value)}
    >
      <span className="knob" />
    </button>
  );
}

/**
 * 步进器：[−] 数值 [+]。点按钮按步长变化，也可以直接输入，回车或离开输入框时生效。
 * 不用滑杆：滑杆一拖就跳很多档，字号、缩放这类设置会瞬间忽大忽小（用户反馈）。
 */
export function Stepper({
  value,
  min,
  max,
  step,
  onChange,
  label,
  unit = "",
  scale = 1,
  digits = 0,
  steps,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange(v: number): void;
  label: string;
  /** 显示在数字后面的单位，比如 "px"、"%" */
  unit?: string;
  /** 显示值 = 实际值 × scale（比如 1.1 显示成 110%） */
  scale?: number;
  digits?: number;
  /** 不等距的档位（整体缩放用浏览器一样的档位） */
  steps?: number[];
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const show = (v: number) => (v * scale).toFixed(digits);
  const clampV = (v: number) => Math.min(max, Math.max(min, v));
  const round = (v: number) => Math.round(v / step) * step;
  const next = (dir: 1 | -1) => {
    if (steps) {
      const list = dir > 0 ? steps : [...steps].reverse();
      return list.find((s) => (dir > 0 ? s > value + 1e-6 : s < value - 1e-6)) ?? value;
    }
    return clampV(+round(value + dir * step).toFixed(6));
  };
  const commit = () => {
    if (draft === null) return;
    const n = Number(draft.replace(/[^\d.\-]/g, "")) / scale;
    if (Number.isFinite(n) && draft.trim() !== "") onChange(clampV(+n.toFixed(6)));
    setDraft(null);
  };
  return (
    <div className="stepper" role="group" aria-label={label}>
      <button type="button" aria-label={`减小${label}`} disabled={value <= min + 1e-9} onClick={() => onChange(next(-1))}>
        −
      </button>
      <label className="stepper-field">
        <input
          inputMode="decimal"
          aria-label={label}
          value={draft ?? show(value)}
          onChange={(e) => setDraft(e.target.value)}
          onFocus={(e) => e.currentTarget.select()}
          onBlur={commit}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") {
              setDraft(null);
              e.currentTarget.blur();
            }
            if (e.key === "ArrowUp" || e.key === "ArrowDown") {
              e.preventDefault();
              setDraft(null);
              onChange(next(e.key === "ArrowUp" ? 1 : -1));
            }
          }}
        />
        {unit && <span className="unit">{unit}</span>}
      </label>
      <button type="button" aria-label={`增大${label}`} disabled={value >= max - 1e-9} onClick={() => onChange(next(1))}>
        +
      </button>
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange(v: T): void;
  label: string;
}) {
  return (
    <div className="seg" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className={o.value === value ? "on" : ""}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Select<T extends string | number>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange(v: T): void;
  label: string;
}) {
  return (
    <select
      className="select"
      aria-label={label}
      value={String(value)}
      onChange={(e) => {
        const o = options.find((x) => String(x.value) === e.target.value);
        if (o) onChange(o.value);
      }}
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

/** 文本框：失去焦点或按回车时才提交，避免每敲一个字就重新加载字体、重启查找 */
export function TextInput({
  value,
  onCommit,
  placeholder,
  label,
  mono,
}: {
  value: string;
  onCommit(v: string): void;
  placeholder?: string;
  label: string;
  mono?: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <input
      className={`text-input${mono ? " mono" : ""}`}
      aria-label={label}
      value={draft ?? value}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft !== null && draft !== value) onCommit(draft);
        setDraft(null);
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          setDraft(null);
          e.currentTarget.blur();
        }
      }}
    />
  );
}
