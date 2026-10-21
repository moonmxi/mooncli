// 设置页用的小控件：开关、滑杆、分段按钮、下拉、文本框，以及一行设置的布局。
import { useId, useState, type ReactNode } from "react";

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

export function Slider({
  value,
  min,
  max,
  step,
  onChange,
  format,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange(v: number): void;
  format(v: number): string;
  label: string;
}) {
  const id = useId();
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <div className="slider">
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={label}
        style={{ "--pct": `${pct}%` } as React.CSSProperties}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <output htmlFor={id}>{format(value)}</output>
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
