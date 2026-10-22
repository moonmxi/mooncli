// 设置页：外观、字体与缩放、终端、标签栏、Shell 与诊断、快捷键、数据与备份。改动立即生效并自动保存。
import { useEffect, useMemo, useRef, useState } from "react";
import { bytesToBase64 } from "../../backend/base64";
import type { Backend, Diagnostics, IntegrationAction, IntegrationStatus, IntegrationTarget, SessionStats } from "../../backend/types";
import { SIDEBAR } from "../../layout/layout";
import {
  DEFAULTS,
  LIMITS,
  chooseTheme,
  effectiveFontSize,
  pickTheme,
  useSettings,
  ZOOM_STEPS,
  type AppSettings,
} from "../../state/settings";
import { useStore } from "../../state/store";
import { useUi, type SettingsSection } from "../../state/ui";
import { BUILTIN_BACKGROUNDS, isBuiltin, renderBuiltin } from "../../theme/backgrounds";
import { builtinThemes, type Theme } from "../../theme/theme";
import { Icon } from "../Icons";
import { Group, Row, Segmented, Select, Stepper, TextInput, Toggle } from "./controls";
import { DataSection } from "./DataSection";
import {
  ACTIONS,
  PRESETS,
  actionDef,
  assign,
  bindingsOf,
  comboFromEvent,
  resetAction,
  shellConflict,
  unassign,
  type ActionId,
  type Keymap,
} from "../../shortcuts";

const SECTIONS: { id: SettingsSection; label: string; icon: string }[] = [
  { id: "appearance", label: "外观", icon: "palette" },
  { id: "font", label: "字体与缩放", icon: "text" },
  { id: "terminal", label: "终端", icon: "pwsh" },
  { id: "tabs", label: "标签栏", icon: "layout" },
  { id: "shell", label: "Shell 与诊断", icon: "info" },
  { id: "agent", label: "Agent 用量", icon: "gauge" },
  { id: "keys", label: "快捷键", icon: "keyboard" },
  { id: "data", label: "数据与备份", icon: "folder" },
];

type Update = (f: (d: AppSettings) => void) => void;

export function SettingsPage({ backend, systemDark }: { backend: Backend; systemDark: boolean }) {
  const open = useUi((s) => s.settings.open);
  const section = useUi((s) => s.settings.section);
  const openSection = useUi((s) => s.openSettings);
  const close = useUi((s) => s.closeSettings);
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) ref.current?.focus();
  }, [open]);

  if (!open) return null;
  const reset = (pick: (d: AppSettings) => void) => (
    <button type="button" className="link-btn" onClick={() => update(pick)}>
      恢复默认
    </button>
  );

  // 居中弹窗：周围留出半透明遮罩，点遮罩、按 Esc 或右上角 × 都能关闭
  return (
    <div className="set-scrim" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        className="settings"
        role="dialog"
        aria-modal="true"
        aria-label="设置"
        tabIndex={-1}
        ref={ref}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          }
        }}
      >
        <header className="set-top">
          <Icon name="gear" />
          <h2>设置</h2>
          <span className="set-saved">改动立即生效，并自动保存</span>
          <button type="button" className="icon-btn set-close" aria-label="关闭设置 (Esc)" title="关闭 (Esc)" onClick={close}>
            <Icon name="close" />
          </button>
        </header>
        <div className="set-main">
          <nav className="set-nav" aria-label="设置分类">
            {SECTIONS.map((sec) => (
              <button
                key={sec.id}
                type="button"
                className={`set-nav-item${section === sec.id ? " on" : ""}`}
                aria-current={section === sec.id}
                onClick={() => openSection(sec.id)}
              >
                <Icon name={sec.icon} />
                <span>{sec.label}</span>
              </button>
            ))}
          </nav>
          <div className="set-body">
          <h3 className="set-section">{SECTIONS.find((x) => x.id === section)?.label}</h3>
          {section === "appearance" && <Appearance s={settings} update={update} backend={backend} systemDark={systemDark} reset={reset} />}
          {section === "font" && <FontAndZoom s={settings} update={update} reset={reset} />}
          {section === "terminal" && <TerminalSection s={settings} update={update} reset={reset} />}
          {section === "tabs" && <TabsSection s={settings} update={update} reset={reset} />}
          {section === "shell" && <ShellSection s={settings} update={update} backend={backend} />}
          {section === "agent" && <AgentSection s={settings} update={update} backend={backend} />}
          {section === "keys" && <KeysSection />}
          {section === "data" && <DataSection backend={backend} />}
          </div>
        </div>
      </div>
    </div>
  );
}

type Reset = (pick: (d: AppSettings) => void) => React.ReactNode;

// ---- 外观 ----

function ThemeCard({ t, active, onPick }: { t: Theme; active: boolean; onPick(): void }) {
  const c = t.terminal;
  return (
    <button type="button" className={`theme-card${active ? " on" : ""}`} onClick={onPick} aria-pressed={active}>
      <span className="theme-preview" style={{ background: c.background, borderColor: t.ui.border }}>
        <span className="tp-bar" style={{ background: t.ui.surface1 }} />
        <span className="tp-line" style={{ color: c.foreground }}>
          <span style={{ color: c.blue }}>~/mooncli</span> <span style={{ color: c.magenta }}>main</span> ❯ ls
        </span>
        <span className="tp-dots">
          {[c.red, c.yellow, c.green, c.cyan, c.blue, c.magenta].map((x) => (
            <i key={x} style={{ background: x }} />
          ))}
        </span>
        <span className="tp-accent" style={{ background: t.ui.accent }} />
      </span>
      <span className="theme-name">
        {active && <Icon name="check" size={14} />}
        {t.displayName}
      </span>
    </button>
  );
}

function Appearance({ s, update, backend, systemDark, reset }: { s: AppSettings; update: Update; backend: Backend; systemDark: boolean; reset: Reset }) {
  const current = pickTheme(s.theme, systemDark);
  const follow = typeof s.theme !== "string" && s.theme.follow === "system";
  const theme = builtinThemes.find((t) => t.name === current) ?? builtinThemes[0];
  const darkThemes = builtinThemes.filter((t) => t.kind === "dark");
  return (
    <>
      <Group title="主题" action={reset((d) => (d.theme = DEFAULTS.theme))}>
        <div className="theme-grid">
          {builtinThemes.map((t) => (
            <ThemeCard key={t.name} t={t} active={t.name === current} onPick={() => update((d) => (d.theme = chooseTheme(d.theme, t.name, t.kind)))} />
          ))}
        </div>
        <Row label="跟随系统深浅色" desc="Windows 切到深色模式时，自动换成夜间主题">
          <Toggle
            label="跟随系统深浅色"
            value={follow}
            onChange={(v) =>
              update((d) => {
                const pair = typeof d.theme === "string" ? { light: builtinThemes.find((t) => t.name === d.theme)?.kind === "light" ? d.theme : "moon-paper", dark: "night-voyage" } : d.theme;
                d.theme = { light: pair.light, dark: pair.dark, follow: v ? "system" : "off" };
              })
            }
          />
        </Row>
        {follow && typeof s.theme !== "string" && (
          <Row label="夜间主题">
            <Select
              label="夜间主题"
              value={s.theme.dark}
              options={darkThemes.map((t) => ({ value: t.name, label: t.displayName }))}
              onChange={(v) => update((d) => typeof d.theme !== "string" && (d.theme.dark = v))}
            />
          </Row>
        )}
      </Group>
      <BackgroundGroup s={s} update={update} backend={backend} theme={theme} reset={reset} />
    </>
  );
}

const POSITIONS = [
  ["top left", "左上"],
  ["top", "上"],
  ["top right", "右上"],
  ["left", "左"],
  ["center", "中"],
  ["right", "右"],
  ["bottom left", "左下"],
  ["bottom", "下"],
  ["bottom right", "右下"],
] as const;

function BackgroundGroup({ s, update, backend, theme, reset }: { s: AppSettings; update: Update; backend: Backend; theme: Theme; reset: Reset }) {
  const [mine, setMine] = useState<{ name: string; url: string }[]>([]);
  const [error, setError] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const builtinThumbs = useMemo(() => BUILTIN_BACKGROUNDS.map((b) => ({ ...b, url: renderBuiltin(b.id, theme, 320, 200) })), [theme]);

  const refresh = () =>
    backend
      .backgroundList()
      .then((names) => Promise.all(names.map((n) => backend.backgroundLoad(n).then((url) => ({ name: n, url })))))
      .then(setMine)
      .catch(() => setMine([]));
  useEffect(() => {
    refresh();
  }, [backend]);

  const onFile = async (f: File | undefined) => {
    if (!f) return;
    setError(null);
    try {
      const b64 = bytesToBase64(new Uint8Array(await f.arrayBuffer()));
      const name = await backend.backgroundSave(f.name, b64);
      await refresh();
      update((d) => (d.background.image = name));
    } catch (e) {
      setError(String(e));
    }
  };

  const bg = s.background;
  const pick = (image: string | null) => update((d) => (d.background.image = image));
  return (
    <Group title="背景图" action={reset((d) => (d.background = { ...DEFAULTS.background }))}>
      <div className="bg-grid">
        <button type="button" className={`bg-card none${bg.image === null ? " on" : ""}`} onClick={() => pick(null)}>
          <span className="bg-thumb" />
          <span>不用背景图</span>
        </button>
        {builtinThumbs.map((b) => (
          <button key={b.id} type="button" className={`bg-card${bg.image === b.id ? " on" : ""}`} onClick={() => pick(b.id)}>
            <span className="bg-thumb" style={{ backgroundImage: `url(${b.url})` }} />
            <span>{b.name}</span>
          </button>
        ))}
        {mine.map((m) => (
          <div key={m.name} className={`bg-card mine${bg.image === m.name ? " on" : ""}`}>
            <button type="button" className="bg-pick" onClick={() => pick(m.name)} title={m.name}>
              <span className="bg-thumb" style={{ backgroundImage: `url(${m.url})` }} />
              <span className="bg-name">{m.name}</span>
            </button>
            <button
              type="button"
              className="bg-del"
              aria-label={`删除 ${m.name}`}
              title="删除这张图"
              onClick={async () => {
                await backend.backgroundDelete(m.name).catch(() => {});
                if (bg.image === m.name) pick(null);
                refresh();
              }}
            >
              <Icon name="trash" size={13} />
            </button>
          </div>
        ))}
        <button type="button" className="bg-card add" onClick={() => fileInput.current?.click()}>
          <span className="bg-thumb">
            <Icon name="plus" size={20} />
          </span>
          <span>添加图片</span>
        </button>
        <input
          ref={fileInput}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif,image/bmp"
          hidden
          onChange={(e) => {
            onFile(e.target.files?.[0]);
            e.target.value = "";
          }}
        />
      </div>
      {error && <div className="set-error">{error}</div>}
      {bg.image !== null && (
        <>
          <Row label="图片不透明度">
            <Stepper label="图片不透明度" value={bg.opacity} min={0.05} max={1} step={0.05} scale={100} unit="%" onChange={(v) => update((d) => (d.background.opacity = v))} />
          </Row>
          <Row label="文字保护" desc="在图片上盖一层和底色相同的半透明层，让文字更清楚">
            <Stepper label="文字保护" value={bg.overlay} min={0} max={0.95} step={0.05} scale={100} unit="%" onChange={(v) => update((d) => (d.background.overlay = v))} />
          </Row>
          <Row label="模糊">
            <Stepper label="模糊" value={bg.blur} min={0} max={30} step={1} unit="px" onChange={(v) => update((d) => (d.background.blur = v))} />
          </Row>
          <Row label="亮度">
            <Stepper label="亮度" value={bg.brightness} min={0.5} max={1.5} step={0.05} scale={100} unit="%" onChange={(v) => update((d) => (d.background.brightness = v))} />
          </Row>
          <Row label="饱和度">
            <Stepper label="饱和度" value={bg.saturation} min={0} max={1.5} step={0.05} scale={100} unit="%" onChange={(v) => update((d) => (d.background.saturation = v))} />
          </Row>
          <Row label="填充方式">
            <Segmented
              label="填充方式"
              value={bg.fit}
              options={[
                { value: "cover", label: "铺满" },
                { value: "contain", label: "完整显示" },
                { value: "tile", label: "平铺" },
                { value: "center", label: "居中" },
              ]}
              onChange={(v) => update((d) => (d.background.fit = v))}
            />
          </Row>
          <Row label="对齐位置" desc={isBuiltin(bg.image) ? "内置图的主体在右下角" : undefined}>
            <div className="pos-grid" role="radiogroup" aria-label="对齐位置">
              {POSITIONS.map(([v, l]) => (
                <button key={v} type="button" role="radio" aria-checked={bg.position === v} aria-label={l} title={l} className={bg.position === v ? "on" : ""} onClick={() => update((d) => (d.background.position = v))}>
                  <i />
                </button>
              ))}
            </div>
          </Row>
        </>
      )}
    </Group>
  );
}

// ---- 字体与缩放 ----

const FONT_PRESETS = [
  ["Cascadia Code", "Windows 自带"],
  ["JetBrains Mono", "需安装"],
  ["Maple Mono NF CN", "中英等宽，需安装"],
  ["Sarasa Mono SC", "中英等宽，需安装"],
  ["Consolas", "Windows 自带"],
] as const;

/** 把字体回退链的第一个换成选中的字体，后面的符号、中文、emoji 字体保留 */
export function withPrimaryFont(family: string, primary: string): string {
  const rest = family
    .split(",")
    .map((f) => f.trim())
    .filter((f) => f && !FONT_PRESETS.some(([p]) => f.replace(/"/g, "") === p));
  return [`"${primary}"`, ...rest].join(", ");
}

const SAMPLE = [
  "PS ~/mooncli> git status  // 0O 1lI {}[] => != >=",
  "😀 🚀 ✅ ⚠️ 👍🏽 👨‍👩‍👧  ← → ↑ ↓ ➜ ❯ ⇒  ┌─┐ └─┘ ░▒▓█",
  "       中文字体对齐：浅色优先的终端",
];

function FontAndZoom({ s, update, reset }: { s: AppSettings; update: Update; reset: Reset }) {
  const primary = s.font.family.split(",")[0]?.replace(/"/g, "").trim();
  return (
    <>
      <Group title="缩放" action={reset((d) => ((d.zoom = 1), (d.uiScale = 1), (d.font.size = DEFAULTS.font.size)))}>
        <Row label="整体缩放" desc="界面和终端文字一起放大缩小。快捷键 Ctrl+= / Ctrl+- / Ctrl+0">
          <div className="zoom-ctl">
            <Stepper label="整体缩放" value={s.zoom} min={LIMITS.zoom.min} max={LIMITS.zoom.max} step={0.05} steps={ZOOM_STEPS} scale={100} unit="%" onChange={(v) => update((d) => (d.zoom = v))} />
            <button type="button" className="link-btn" disabled={s.zoom === 1} onClick={() => update((d) => (d.zoom = 1))}>
              重置
            </button>
          </div>
        </Row>
        <Row label="界面尺寸" desc="只放大标题栏、标签栏、状态栏和各种面板，终端文字不变">
          <Stepper label="界面尺寸" value={s.uiScale} min={LIMITS.uiScale.min} max={LIMITS.uiScale.max} step={0.05} scale={100} unit="%" onChange={(v) => update((d) => (d.uiScale = v))} />
        </Row>
        <Row label="终端字号" desc={`只调终端里的文字，界面不变。在终端里按住 Ctrl 滚动滚轮也可以调${s.zoom !== 1 ? `（乘上整体缩放后实际是 ${effectiveFontSize(s)}px）` : ""}`}>
          <Stepper label="终端字号" value={s.font.size} min={LIMITS.fontSize.min} max={LIMITS.fontSize.max} step={0.5} digits={1} unit="px" onChange={(v) => update((d) => (d.font.size = v))} />
        </Row>
      </Group>
      <Group title="终端字体" action={reset((d) => (d.font = { ...DEFAULTS.font, size: d.font.size }))}>
        <Row label="字体" desc="选一个常用字体，或者在下面直接填写" wide>
          <div className="chips">
            {FONT_PRESETS.map(([f, note]) => (
              <button key={f} type="button" className={`chip${primary === f ? " on" : ""}`} title={note} onClick={() => update((d) => (d.font.family = withPrimaryFont(d.font.family, f)))}>
                <span style={{ fontFamily: `"${f}", monospace` }}>{f}</span>
              </button>
            ))}
          </div>
        </Row>
        <Row label="字体回退链" desc="按顺序找字：英文 → 图标 → 中文 → emoji → 符号。用逗号分隔" wide>
          <TextInput label="字体回退链" mono value={s.font.family} onCommit={(v) => update((d) => (d.font.family = v.trim() || DEFAULTS.font.family))} />
        </Row>
        <Row label="行高">
          <Stepper label="行高" value={s.font.lineHeight} min={LIMITS.lineHeight.min} max={LIMITS.lineHeight.max} step={0.05} digits={2} onChange={(v) => update((d) => (d.font.lineHeight = v))} />
        </Row>
        <Row label="字重">
          <Segmented
            label="字重"
            value={s.font.weight}
            options={[
              { value: "300", label: "细" },
              { value: "normal", label: "常规" },
              { value: "500", label: "中等" },
              { value: "600", label: "半粗" },
            ]}
            onChange={(v) => update((d) => (d.font.weight = v))}
          />
        </Row>
        <div className="font-preview" style={{ fontFamily: s.font.family, fontSize: effectiveFontSize(s), lineHeight: s.font.lineHeight, fontWeight: s.font.weight }}>
          {SAMPLE.map((l) => (
            <div key={l}>{l}</div>
          ))}
        </div>
      </Group>
    </>
  );
}

// ---- 终端 ----

function TerminalSection({ s, update, reset }: { s: AppSettings; update: Update; reset: Reset }) {
  const t = s.terminal;
  const set = <K extends keyof AppSettings["terminal"]>(k: K) => (v: AppSettings["terminal"][K]) => update((d) => (d.terminal[k] = v));
  return (
    <>
      <Group title="光标与滚动" action={reset((d) => Object.assign(d.terminal, { cursorStyle: DEFAULTS.terminal.cursorStyle, cursorBlink: DEFAULTS.terminal.cursorBlink, scrollback: DEFAULTS.terminal.scrollback, smoothScrollMs: DEFAULTS.terminal.smoothScrollMs, minimumContrastRatio: DEFAULTS.terminal.minimumContrastRatio }))}>
        <Row label="光标样式">
          <Segmented label="光标样式" value={t.cursorStyle} options={[{ value: "bar", label: "竖线" }, { value: "block", label: "方块" }, { value: "underline", label: "下划线" }]} onChange={set("cursorStyle")} />
        </Row>
        <Row label="光标闪烁">
          <Toggle label="光标闪烁" value={t.cursorBlink} onChange={set("cursorBlink")} />
        </Row>
        <Row label="回滚行数" desc="往上能翻多少行。行数越多越占内存">
          <Select label="回滚行数" value={t.scrollback} options={[1000, 5000, 10000, 50000, 100000].map((n) => ({ value: n, label: `${n.toLocaleString()} 行` }))} onChange={set("scrollback")} />
        </Row>
        <Row label="平滑滚动" desc="鼠标滚轮滚动时带一点缓动">
          <Toggle label="平滑滚动" value={t.smoothScrollMs > 0} onChange={(v) => set("smoothScrollMs")(v ? 120 : 0)} />
        </Row>
        <Row label="自动加深看不清的文字" desc="程序输出了在当前背景上看不清的颜色时，自动加深到这个对比度。1 表示不处理。设太高会让联想提示和正文一样深，难以分辨">
          <Stepper label="最低对比度" value={t.minimumContrastRatio} min={1} max={7} step={0.5} digits={1} unit=": 1" onChange={set("minimumContrastRatio")} />
        </Row>
      </Group>
      <Group title="复制与粘贴" action={reset((d) => Object.assign(d.terminal, { rightClick: DEFAULTS.terminal.rightClick, copyOnSelect: DEFAULTS.terminal.copyOnSelect, copyToast: DEFAULTS.terminal.copyToast, confirmMultilinePaste: DEFAULTS.terminal.confirmMultilinePaste, trimTrailingWhitespace: DEFAULTS.terminal.trimTrailingWhitespace }))}>
        <Row label="右键" desc="按住 Shift 右键总是弹出菜单">
          <Segmented label="右键" value={t.rightClick} options={[{ value: "copyPaste", label: "复制 / 粘贴" }, { value: "menu", label: "弹出菜单" }]} onChange={set("rightClick")} />
        </Row>
        <Row label="选中即复制">
          <Toggle label="选中即复制" value={t.copyOnSelect} onChange={set("copyOnSelect")} />
        </Row>
        <Row label="复制提醒" desc="复制后在鼠标旁边弹出一张小卡片">
          <Toggle label="复制提醒" value={t.copyToast} onChange={set("copyToast")} />
        </Row>
        <Row label="多行粘贴前确认" desc="程序没开括号粘贴时，多行内容可能被逐行执行">
          <Toggle label="多行粘贴前确认" value={t.confirmMultilinePaste} onChange={set("confirmMultilinePaste")} />
        </Row>
        <Row label="复制时去掉行尾空格">
          <Toggle label="复制时去掉行尾空格" value={t.trimTrailingWhitespace} onChange={set("trimTrailingWhitespace")} />
        </Row>
      </Group>
    </>
  );
}

// ---- 标签栏 ----

function TabsSection({ s, update, reset }: { s: AppSettings; update: Update; reset: Reset }) {
  return (
    <Group title="标签栏" action={reset((d) => (d.tabs = structuredClone(DEFAULTS.tabs)))}>
      <Row label="位置">
        <Segmented label="标签栏位置" value={s.tabs.position} options={[{ value: "left", label: "左侧" }, { value: "top", label: "顶部" }]} onChange={(v) => update((d) => (d.tabs.position = v))} />
      </Row>
      <Row label="侧栏宽度" desc="52 表示收起。也可以直接拖动侧栏右边缘，双击边缘切换收起和展开">
        <Stepper
          label="侧栏宽度"
          value={s.tabs.sidebar.width}
          min={SIDEBAR.collapsed}
          max={SIDEBAR.max}
          step={8}
          unit="px"
          onChange={(v) => update((d) => (d.tabs.sidebar.width = v < SIDEBAR.min ? (v < d.tabs.sidebar.width ? SIDEBAR.collapsed : SIDEBAR.min) : v))}
        />
      </Row>
      <Row label="窗口变窄时自动收起侧栏" desc="窗口宽度不够时先收起侧栏，保证终端至少 80 列">
        <Toggle label="自动收起侧栏" value={s.tabs.sidebar.autoCollapse} onChange={(v) => update((d) => (d.tabs.sidebar.autoCollapse = v))} />
      </Row>
    </Group>
  );
}

// ---- Agent 用量 ----

function AgentSection({ s, update, backend }: { s: AppSettings; update: Update; backend: Backend }) {
  const [bridgePath, setBridgePath] = useState("");
  useEffect(() => {
    backend.agentBridgePath().then(setBridgePath).catch(() => {});
  }, [backend]);
  const a = s.agent;
  return (
    <>
      <Group title="用量小窗">
        <Row
          label="显示 Agent 用量小窗"
          desc="在窗口角落浮动显示 Claude Code / Codex 的上下文占用和账号额度（5 小时、周）。数据只来自它们官方扩展点送来的内容和它们自己写在本机的会话文件——不读任何登录凭证，不发网络请求（docs/07）。"
        >
          <Toggle label="Agent 用量小窗" value={a.enabled} onChange={(v) => update((d) => (d.agent.enabled = v))} />
        </Row>
      </Group>
      <Group title="数据来源">
        <Row label="读取会话文件" desc="读 Claude Code 的会话转录和 Codex 的 rollout 文件：上下文占用、Codex 的额度快照、Claude 的用量估算都从这里来。只读，不改。">
          <Toggle label="读取会话文件" value={a.readSessionFiles} onChange={(v) => update((d) => (d.agent.readSessionFiles = v))} />
        </Row>
        <Row label="接收 statusLine 数据" desc="接入 Claude Code 的 statusLine 后（见下方），每次刷新都会把当前会话信息送给 mooncli：模型、上下文、花费，支持时还有官方额度百分比。">
          <Toggle label="接收 statusLine 数据" value={a.receiveStatusline} onChange={(v) => update((d) => (d.agent.receiveStatusline = v))} />
        </Row>
        <Row label="接收 hooks 状态" desc="接入 hooks 后，小窗能显示 Agent 是在运行、等你确认还是本轮已完成。">
          <Toggle label="接收 hooks 状态" value={a.receiveHooks} onChange={(v) => update((d) => (d.agent.receiveHooks = v))} />
        </Row>
      </Group>
      <Group title="小窗外观">
        <Row label="停靠角落">
          <Segmented
            label="小窗停靠角落"
            value={a.widget.anchor}
            options={[
              { value: "bottom-right", label: "右下" },
              { value: "bottom-left", label: "左下" },
              { value: "top-right", label: "右上" },
              { value: "top-left", label: "左上" },
            ]}
            onChange={(v) => update((d) => (d.agent.widget.anchor = v))}
          />
        </Row>
        <Row label="不透明度">
          <Stepper label="小窗不透明度" value={Math.round(a.widget.opacity * 100)} min={50} max={100} step={5} unit="%" onChange={(v) => update((d) => (d.agent.widget.opacity = v / 100))} />
        </Row>
        <Row label="警告阈值" desc="用量达到这个比例时进度条变警告色">
          <Stepper label="警告阈值" value={Math.round(a.widget.warnAt * 100)} min={50} max={90} step={5} unit="%" onChange={(v) => update((d) => (d.agent.widget.warnAt = v / 100))} />
        </Row>
        <Row label="危险阈值" desc="用量达到这个比例时进度条变危险色、数字加粗">
          <Stepper label="危险阈值" value={Math.round(a.widget.dangerAt * 100)} min={80} max={99} step={1} unit="%" onChange={(v) => update((d) => (d.agent.widget.dangerAt = v / 100))} />
        </Row>
      </Group>
      <IntegrationGroup backend={backend} bridgePath={bridgePath} />
    </>
  );
}

function IntegrationGroup({ backend, bridgePath }: { backend: Backend; bridgePath: string }) {
  const [status, setStatus] = useState<IntegrationStatus | null>(null);
  const [pending, setPending] = useState<{ target: IntegrationTarget; action: IntegrationAction; changes: string[] } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = () => {
    backend.agentIntegrationStatus().then(setStatus).catch(() => {});
  };
  useEffect(refresh, [backend]);

  const preview = (target: IntegrationTarget, action: IntegrationAction) => {
    setNote(null);
    backend
      .agentIntegrationPreview(target, action)
      .then((o) => setPending({ target, action, changes: o.changes }))
      .catch((e) => setNote(`预览失败：${String(e)}`));
  };
  const apply = () => {
    if (!pending) return;
    setBusy(true);
    backend
      .agentIntegrationApply(pending.target, pending.action)
      .then((o) => {
        setNote(`已写入（原文件已自动备份）：${o.changes.join("；")}`);
        setPending(null);
        refresh();
      })
      .catch((e) => setNote(`写入失败：${String(e)}`))
      .finally(() => setBusy(false));
  };

  const targetRow = (target: IntegrationTarget, label: string, integrated: boolean | undefined, path: string, desc: string) => (
    <Row
      label={label}
      desc={
        <>
          {desc}
          <br />
          配置文件：<code>{path}</code> · 状态：{integrated === undefined ? "读取中" : integrated ? "已接入" : "未接入"}
        </>
      }
    >
      <button type="button" className="btn" disabled={integrated === undefined || busy} onClick={() => preview(target, integrated ? "unintegrate" : "integrate")}>
        {integrated ? "断开" : "接入"}
      </button>
    </Row>
  );

  return (
    <Group title="接入（让数据更实时）">
      <Row
        label="说明"
        desc={
          <>
            不接入也能用（只读会话文件）。接入是把官方扩展点指向 mooncli-bridge
            {bridgePath ? (
              <>
                （<code>{bridgePath}</code>）
              </>
            ) : null}
            ：你原来的 statusLine / notify 会被保存并由 bridge 原样转调，显示和行为不变；每次写入前自动备份原文件，断开时精确恢复。
          </>
        }
      >
        <span />
      </Row>
      {targetRow("claude", "Claude Code", status?.claude, status?.claudeSettingsPath ?? "~/.claude/settings.json", "statusLine + hooks：会话状态（等确认/已完成）和官方额度实时进来。")}
      {targetRow("codex", "Codex", status?.codex, status?.codexConfigPath ?? "~/.codex/config.toml", "notify：一轮结束时触发小窗刷新（用量仍读会话文件，notify 本身不带用量）。")}
      {pending && (
        <Row
          label={pending.action === "integrate" ? "确认接入" : "确认断开"}
          desc={
            <ul style={{ margin: 0, paddingLeft: 16 }}>
              {pending.changes.map((ch) => (
                <li key={ch}>{ch}</li>
              ))}
            </ul>
          }
        >
          <span style={{ display: "inline-flex", gap: 8 }}>
            <button type="button" className="btn primary" disabled={busy} onClick={apply}>
              确认写入
            </button>
            <button type="button" className="btn" disabled={busy} onClick={() => setPending(null)}>
              取消
            </button>
          </span>
        </Row>
      )}
      {note && (
        <Row label="结果" desc={note}>
          <span />
        </Row>
      )}
    </Group>
  );
}

// ---- Shell 与诊断 ----

function ShellSection({ s, update, backend }: { s: AppSettings; update: Update; backend: Backend }) {
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [stats, setStats] = useState<SessionStats | null>(null);
  const tab = useStore((st) => st.tabs.find((t) => t.id === st.activeId));
  useEffect(() => {
    backend.diagnostics().then(setDiag).catch(() => setDiag(null));
    if (tab) backend.sessionStats(tab.id).then(setStats).catch(() => setStats(null));
  }, [backend, s.shell, tab?.id]);
  const integrated = tab && tab.shellState !== "unknown";
  const reason = diagnose(integrated, stats, diag);
  const lines = diag
    ? [
        ["mooncli 版本", diag.appVersion],
        ["系统", `${diag.os}${diag.osVersion ? ` ${diag.osVersion}` : ""}`],
        ["PowerShell", diag.shellPath ?? "没有找到"],
        ["shell 集成", diag.integrationEnabled ? "已开启" : "已关闭"],
        ["当前标签页", integrated ? "已集成" : "未集成"],
        ...(stats
          ? [["收到的集成标记", `本机 ${stats.trustedMarks} · 其它 ${stats.untrustedMarks} · 私有消息 ${stats.privateMessages} · 输出 ${Math.round(stats.outputBytes / 1024)} KB`]]
          : []),
        ["ConPTY", diag.bundledConpty ? "使用附带的新版" : "使用系统自带"],
        ["设置文件", diag.settingsPath],
        ["历史库", `${diag.historyPath}${diag.historyCount !== null ? `（${diag.historyCount} 条）` : ""}`],
        ["背景图目录", diag.backgroundsDir],
      ]
    : [];
  const [copied, setCopied] = useState(false);
  return (
    <>
      <Group title="PowerShell">
        <Row label="pwsh 路径" desc="留空时自动查找（PATH、Program Files、Microsoft Store 版）" wide>
          <TextInput label="pwsh 路径" mono value={s.shell.path ?? ""} placeholder="自动查找" onCommit={(v) => update((d) => (d.shell.path = v.trim() || null))} />
        </Row>
        <Row label="启动参数" desc="比如 -NoProfile。用空格分隔，新开的标签页生效" wide>
          <TextInput label="启动参数" mono value={s.shell.args.join(" ")} placeholder="（无）" onCommit={(v) => update((d) => (d.shell.args = v.split(/\s+/).filter(Boolean)))} />
        </Row>
        <Row label="shell 集成" desc="让 mooncli 知道提示符、命令、目录和退出码。关掉后终端照常可用，但没有智能功能。新开的标签页生效">
          <Toggle label="shell 集成" value={s.shell.integration} onChange={(v) => update((d) => (d.shell.integration = v))} />
        </Row>
        <Row label="配色跟随主题" desc="把 PSReadLine 和 PowerShell 自带的配色换成跟随主题的颜色，让输入的文字和灰色联想提示分得清。只替换你没改过的颜色。新开的标签页生效">
          <Toggle label="配色跟随主题" value={s.shell.adaptColors} onChange={(v) => update((d) => (d.shell.adaptColors = v))} />
        </Row>
        <Row label="使用附带的 ConPTY" desc="用安装包里附带的新版 ConPTY，老版本 Windows 自带的会吞掉集成标记。重启 mooncli 生效">
          <Toggle label="使用附带的 ConPTY" value={s.shell.useBundledConpty} onChange={(v) => update((d) => (d.shell.useBundledConpty = v))} />
        </Row>
      </Group>
      <Group title="连接服务器">
        <div className="set-hint">
          运行 ssh、sshp、sshpass、mosh、wsl、docker exec -it 等命令时，连上就算这条命令完成，之后单独记录在服务器上执行的每条命令。自己写的 ssh 别名或脚本，可以加在下面。
        </div>
        <Row label="也算连接的命令" desc="写命令名，用逗号或空格隔开。参数按 ssh 的写法识别，比如 jump -l root prod-1">
          <TextInput
            label="也算连接的命令"
            mono
            placeholder="比如 jump, myssh"
            value={s.remote.connectCommands.join(", ")}
            onCommit={(v) =>
              update((d) => (d.remote.connectCommands = [...new Set(v.split(/[\s,，]+/).map((x) => x.trim()).filter((x) => /^[\w.-]{1,40}$/.test(x)))]))
            }
          />
        </Row>
      </Group>
      <Group
        title="诊断"
        action={
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              backend.clipboardWrite(lines.map(([k, v]) => `${k}: ${v}`).join("\n")).then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              });
            }}
          >
            {copied ? "已复制" : "复制诊断信息"}
          </button>
        }
      >
        {!integrated && tab && <div className="set-hint">{reason}</div>}
        <dl className="diag">
          {lines.map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>{v}</dd>
            </div>
          ))}
        </dl>
      </Group>
    </>
  );
}

/** 根据收到的信号推断"未集成"的原因 */
export function diagnose(integrated: boolean | undefined, stats: SessionStats | null, diag: Diagnostics | null): string {
  if (integrated) return "";
  if (diag && !diag.integrationEnabled) return "shell 集成在上面被关掉了。打开后新开一个标签页生效。";
  if (!stats || stats.outputBytes === 0) return "当前标签页还没有任何输出，稍等一下再看。";
  if (stats.untrustedMarks > 0 && stats.trustedMarks === 0)
    return "收到了集成标记，但不是本机 pwsh 发出的（可能是在里面又启动了一个 shell，或者 ssh 到了远程）。新开一个标签页看看。";
  if (stats.trustedMarks === 0 && stats.privateMessages === 0) {
    if (diag && !diag.bundledConpty)
      return "一个集成标记都没收到，而且现在用的是系统自带的 ConPTY。Windows 10 和较早的 Windows 11 自带的 ConPTY 会把这些标记吞掉。请安装附带新版 ConPTY 的安装包，或在上面打开“使用附带的 ConPTY”后重启 mooncli。";
    return "一个集成标记都没收到。新开一个标签页，看提示符上方有没有黄色的“shell 集成没有加载成功”提示；有的话把那句话发给开发者。也可能是 pwsh 太旧（需要 7.2 以上）。";
  }
  return "收到过集成信号，但当前不在等待输入的状态（比如正在运行程序）。";
}

// ---- 快捷键 ----

/** 固定的按键：不能改，列出来方便查 */
const FIXED_KEYS: [string, string][] = [
  ["跳到第 N 个标签页", "Alt+1 … Alt+9"],
  ["调终端字号", "Ctrl+滚轮"],
  ["复制（有选中时）", "Ctrl+C"],
  ["总是复制", "Ctrl+Shift+C"],
  ["粘贴", "Ctrl+V / Shift+Insert"],
  ["右键菜单", "Shift+右键"],
  ["重命名（选中侧栏里的标签页时）", "F2"],
];

/** 历史面板里的按键（面板打开时才有效，不会挡住 shell） */
const HISTORY_KEYS: [string, string][] = [
  ["填入 / 直接运行", "Enter / Shift+Enter"],
  ["复制、收藏", "Ctrl+C、Ctrl+S"],
  ["在这条命令的目录打开新标签页", "Ctrl+O"],
  ["删除（按两次）", "Shift+Del"],
  ["切换范围：全部 / 当前目录 / 本标签页 / 收藏", "Alt+1 … Alt+4"],
  ["合并重复", "Alt+D"],
  ["详情（窄窗口）", "Tab"],
];

function KeysSection() {
  const keys = useSettings((s) => s.settings.keys);
  const update = useSettings((s) => s.update);
  const setRecordingKey = useUi((s) => s.setRecordingKey);
  const [recording, setRecording] = useState<ActionId | null>(null);
  const [message, setMessage] = useState<{ id: ActionId; text: string } | null>(null);

  const setKeys = (k: Keymap) => update((d) => (d.keys = k));

  // 录键：在捕获阶段拿到按键，不让它触发任何快捷键，也不交给终端
  useEffect(() => {
    setRecordingKey(recording !== null);
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        setRecording(null);
        return;
      }
      const combo = comboFromEvent(e);
      if (!combo) return; // 只按了修饰键，继续等
      const { keymap, takenFrom } = assign(useSettings.getState().settings.keys, recording, combo);
      setKeys(keymap);
      const notes = [
        takenFrom && `已从"${actionDef(takenFrom).label}"那里拿过来`,
        shellConflict(combo) && `注意：${combo} 在终端里原本是"${shellConflict(combo)}"，设成快捷键后这个功能就用不了了`,
      ].filter(Boolean);
      setMessage(notes.length ? { id: recording, text: notes.join("；") } : null);
      setRecording(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      setRecordingKey(false);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording]);

  const presetOn = PRESETS.find((p) => JSON.stringify(p.keys) === JSON.stringify(keys))?.id;
  const groups = [...new Set(ACTIONS.map((a) => a.group))];

  return (
    <>
      <Group title="预设">
        <div className="key-presets">
          {PRESETS.map((p) => (
            <button key={p.id} type="button" className={`key-preset${presetOn === p.id ? " on" : ""}`} onClick={() => setKeys(p.keys)}>
              <strong>{p.label}</strong>
              <span>{p.desc}</span>
            </button>
          ))}
        </div>
        <div className="set-hint">
          快捷键在任何时候都由 mooncli 处理，其它按键原样交给 shell。所以默认都用 Ctrl+Shift 组合，不占用 shell、vim、tmux 里的键；想用 Ctrl+B 这类更顺手的键也可以，设的时候会提醒它原本的用途。
        </div>
      </Group>
      {groups.map((g) => (
        <Group key={g} title={g}>
          {ACTIONS.filter((a) => a.group === g).map((a) => {
            const list = bindingsOf(a.id, keys);
            const changed = keys[a.id] !== undefined;
            return (
              <Row key={a.id} label={a.label} desc={message?.id === a.id ? <span className="key-warn">{message.text}</span> : undefined}>
                <div className="key-row">
                  {list.map((k) => (
                    <span key={k} className={`key-chip${shellConflict(k) ? " risky" : ""}`} title={shellConflict(k) ? `在终端里原本是：${shellConflict(k)}` : undefined}>
                      <kbd>{k}</kbd>
                      <button type="button" aria-label={`去掉 ${k}`} onClick={() => setKeys(unassign(keys, a.id, k))}>
                        <Icon name="close" size={11} />
                      </button>
                    </span>
                  ))}
                  {list.length === 0 && <span className="key-none">未设置</span>}
                  <button
                    type="button"
                    className={`btn key-add${recording === a.id ? " recording" : ""}`}
                    onClick={() => {
                      setMessage(null);
                      setRecording(recording === a.id ? null : a.id);
                    }}
                  >
                    {recording === a.id ? "请按下新的快捷键…（Esc 取消）" : "+ 添加"}
                  </button>
                  {changed && (
                    <button type="button" className="link-btn" title={`恢复为 ${a.defaults.join(" / ") || "未设置"}`} onClick={() => setKeys(resetAction(keys, a.id))}>
                      恢复默认
                    </button>
                  )}
                </div>
              </Row>
            );
          })}
        </Group>
      ))}
      <Group title="固定的按键">
        <dl className="keys">
          {FIXED_KEYS.map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>
                <kbd>{v}</kbd>
              </dd>
            </div>
          ))}
        </dl>
      </Group>
      <Group title="历史面板里">
        <dl className="keys">
          {HISTORY_KEYS.map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>
                <kbd>{v}</kbd>
              </dd>
            </div>
          ))}
        </dl>
      </Group>
    </>
  );
}

