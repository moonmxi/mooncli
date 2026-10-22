// 设置 → Agent（docs/07）：接入 Claude Code / Codex 的官方扩展点、完成提醒、额度卡片。
import { useEffect, useState } from "react";
import type { AppSettings } from "../../state/settings";
import type { Backend, IntegrationAction, IntegrationStatus, IntegrationTarget } from "../../backend/types";
import { claudeSnippet, codexSnippet } from "../../agent/manual";
import { useUi } from "../../state/ui";
import { Group, Row, Segmented, Stepper, TextInput, Toggle } from "./controls";

type Update = (f: (d: AppSettings) => void) => void;

const TARGET_LABEL: Record<IntegrationTarget, string> = { claude: "Claude Code", codex: "Codex" };

export function AgentSection({ s, update, backend }: { s: AppSettings; update: Update; backend: Backend }) {
  const [bridgePath, setBridgePath] = useState("");
  useEffect(() => {
    backend.agentBridgePath().then(setBridgePath).catch(() => {});
  }, [backend]);
  const a = s.agent;
  const testToast = () =>
    backend
      .toast("mooncli", "Claude Code / Codex 一轮结束时，会像这样提醒你")
      .catch((e) => useUi.getState().showNotice(`通知没有发出去：${String(e)}`));
  return (
    <>
      <IntegrationGroup backend={backend} bridgePath={bridgePath} />
      <Group title="完成提醒">
        <Row
          label="一轮结束时提醒"
          desc="Claude Code、Codex 一轮跑完或要你确认（Claude Code 要权限）时，右下角弹一条系统通知，点一下回到那个标签页。需要先在上面接入。"
        >
          <Toggle label="完成提醒" value={a.notify.enabled} onChange={(v) => update((d) => (d.agent.notify.enabled = v))} />
        </Row>
        <Row label="正看着时不提醒" desc="你正看着运行 Agent 的那个标签页（或别的终端窗口）时不弹，只在你切走以后提醒。">
          <Toggle label="正看着时不提醒" value={a.notify.quietWhenFocused} onChange={(v) => update((d) => (d.agent.notify.quietWhenFocused = v))} />
        </Row>
        <Row label="试一下" desc="发一条测试通知。没看到的话，检查 Windows 设置 → 系统 → 通知里 mooncli 和「请勿打扰」。">
          <button type="button" className="btn" onClick={testToast}>
            发送测试通知
          </button>
        </Row>
      </Group>
      <Group title="额度卡片">
        <Row
          label="显示 Agent 额度"
          desc="列出 Claude Code 和 Codex 各自的 5 小时、7 天额度；当前标签页正在跑其中一个时，再显示这个会话的上下文占用。只用官方渠道（扩展点、Codex 自己的 app-server、本机会话文件），不读任何登录凭证。"
        >
          <Toggle label="Agent 额度" value={a.enabled} onChange={(v) => update((d) => (d.agent.enabled = v))} />
        </Row>
        <Row label="浮动卡片" desc="停在窗口一角，按住标题可以拖到别的角。">
          <Segmented
            label="浮动卡片"
            value={a.widget.style}
            options={[
              { value: "floating", label: "显示" },
              { value: "off", label: "不显示" },
            ]}
            onChange={(v) => update((d) => (d.agent.widget.style = v))}
          />
        </Row>
        <Row label="状态栏摘要" desc="状态栏右侧显示「Claude 46% · 6%   Codex 1% · 0%」（5 小时 · 7 天），点击可以临时收起浮动卡片。">
          <Toggle label="状态栏摘要" value={a.widget.statusBar} onChange={(v) => update((d) => (d.agent.widget.statusBar = v))} />
        </Row>
        <Row label="独立小窗" desc="单独的置顶小窗口，切到别的程序时也能看到额度。命令面板里也能打开。">
          <button type="button" className="btn" disabled={!a.enabled} onClick={() => backend.agentWindow(true).catch((e) => useUi.getState().showNotice(String(e)))}>
            打开
          </button>
        </Row>
        <Row
          label="Codex 实时额度"
          desc="每两分钟通过本机 Codex 的 app-server（Codex IDE 插件用的官方接口）查一次，不用等下一轮对话。登录和联网都由 Codex 自己完成。关掉时用会话文件里最近一次的记录。"
        >
          <Toggle label="Codex 实时额度" value={a.liveQuota} onChange={(v) => update((d) => (d.agent.liveQuota = v))} />
        </Row>
        <Row label="codex 位置" desc="留空时自动在 PATH 里找（npm 装的会找到包里自带的 codex.exe）。">
          <TextInput label="codex 位置" mono value={a.codexPath ?? ""} placeholder="自动" onCommit={(v) => update((d) => (d.agent.codexPath = v.trim() || null))} />
        </Row>
      </Group>
      <Group title="卡片外观">
        <Row label="停靠角落">
          <Segmented
            label="卡片停靠角落"
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
          <Stepper label="卡片不透明度" value={Math.round(a.widget.opacity * 100)} min={50} max={100} step={5} unit="%" onChange={(v) => update((d) => (d.agent.widget.opacity = v / 100))} />
        </Row>
        <Row label="警告阈值" desc="用量达到这个比例时进度条变警告色">
          <Stepper label="警告阈值" value={Math.round(a.widget.warnAt * 100)} min={50} max={90} step={5} unit="%" onChange={(v) => update((d) => (d.agent.widget.warnAt = v / 100))} />
        </Row>
        <Row label="危险阈值" desc="用量达到这个比例时进度条变危险色、数字加粗">
          <Stepper label="危险阈值" value={Math.round(a.widget.dangerAt * 100)} min={80} max={99} step={1} unit="%" onChange={(v) => update((d) => (d.agent.widget.dangerAt = v / 100))} />
        </Row>
      </Group>
    </>
  );
}

function IntegrationGroup({ backend, bridgePath }: { backend: Backend; bridgePath: string }) {
  const [status, setStatus] = useState<IntegrationStatus | null>(null);
  const [pending, setPending] = useState<{ target: IntegrationTarget; action: IntegrationAction; changes: string[] } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [manual, setManual] = useState(false);

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
        setNote(`${TARGET_LABEL[pending.target]}：已写入（原文件已自动备份）。${o.changes.join("；")}`);
        setPending(null);
        refresh();
      })
      .catch((e) => setNote(`写入失败：${String(e)}`))
      .finally(() => setBusy(false));
  };
  const copy = (text: string) =>
    backend.clipboardWrite(text).then(
      () => useUi.getState().showNotice("已复制"),
      () => {},
    );

  const row = (target: IntegrationTarget, what: string, path: string | undefined) => {
    const on = status?.[target];
    return (
      <Row
        label={TARGET_LABEL[target]}
        desc={
          <>
            {what}
            <br />
            配置文件：<code>{path ?? "读取中"}</code> · {on === undefined ? "读取中" : on ? "已接入" : "未接入"}
          </>
        }
      >
        <button
          type="button"
          className={`btn${on ? "" : " primary"}`}
          disabled={on === undefined || busy || (!on && status?.bridgeExists === false)}
          onClick={() => preview(target, on ? "unintegrate" : "integrate")}
        >
          {on ? "断开" : "接入"}
        </button>
      </Row>
    );
  };

  return (
    <Group title="接入 Claude Code 与 Codex">
      <Row
        label="说明"
        desc={
          <>
            把两者的官方扩展点指向 mooncli-bridge，额度和完成提醒才是实时的。你原来的 statusLine、notify 会保存下来由 bridge 原样转调，hooks 只追加不覆盖；写入前先给你看变更清单并自动备份，断开时精确恢复。也可以
            <button type="button" className="link-btn" onClick={() => setManual((v) => !v)}>
              {manual ? "收起手动配置" : "自己手动配置"}
            </button>
            。
            {status?.bridgeExists === false && <strong>（没找到 mooncli-bridge，开发版请先运行 node scripts/build-bridge.mjs）</strong>}
          </>
        }
      >
        <span />
      </Row>
      {row("claude", "statusLine 送来 5 小时 / 7 天额度（Pro / Max 订阅）和上下文；hooks 在一轮结束、要你确认时提醒。没有原状态栏时，bridge 在 Claude Code 底部显示一行「模型 · 上下文 · 额度」。", status?.claudeSettingsPath)}
      {row("codex", "notify：一轮结束时提醒，并把会话关联到标签页。额度来自 Codex 的 app-server，不依赖接入。", status?.codexConfigPath)}
      {pending && (
        <Row
          label={`${pending.action === "integrate" ? "确认接入" : "确认断开"} ${TARGET_LABEL[pending.target]}`}
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
      {manual && bridgePath && (
        <>
          <Row
            wide
            label="手动配置 Claude Code"
            desc={
              <>
                合并进 <code>{status?.claudeSettingsPath ?? "~/.claude/settings.json"}</code>（已有 statusLine 的话会替换掉它；已有 hooks 的话把这几条追加进对应事件）：
                <pre className="snippet">{claudeSnippet(bridgePath)}</pre>
              </>
            }
          >
            <button type="button" className="btn small" onClick={() => copy(claudeSnippet(bridgePath))}>
              复制
            </button>
          </Row>
          <Row
            wide
            label="手动配置 Codex"
            desc={
              <>
                写在 <code>{status?.codexConfigPath ?? "~/.codex/config.toml"}</code> 顶层（第一个 <code>[表]</code> 之前；已有 notify 的话替换它）：
                <pre className="snippet">{codexSnippet(bridgePath)}</pre>
              </>
            }
          >
            <button type="button" className="btn small" onClick={() => copy(codexSnippet(bridgePath))}>
              复制
            </button>
          </Row>
        </>
      )}
    </Group>
  );
}
