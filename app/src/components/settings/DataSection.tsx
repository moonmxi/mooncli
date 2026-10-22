// 设置 → 数据：存在哪、占多大，设置的自动备份和恢复，导出导入，背景图和命令历史的清理（docs/03 §7）。
import { useCallback, useEffect, useRef, useState } from "react";
import { bytesToBase64 } from "../../backend/base64";
import type { Backend, DataOverview } from "../../backend/types";
import { fullTime } from "../../history/format";
import { mergeSettings, useSettings } from "../../state/settings";
import { useUi } from "../../state/ui";
import { Group, Row, Select, Toggle } from "./controls";

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

const REASONS: Record<string, string> = {
  auto: "每天自动",
  manual: "手动",
  "before-import": "导入之前",
  "before-restore": "恢复之前",
  "before-reset": "重置之前",
};

const KEEP_OPTIONS = [
  { value: 0, label: "一直保留" },
  { value: 30, label: "最近 30 天" },
  { value: 90, label: "最近 90 天" },
  { value: 180, label: "最近半年" },
  { value: 365, label: "最近一年" },
];

/** 危险操作点两次才执行：第一次按钮变成"再点一次…"，3 秒内没再点就恢复 */
function useConfirm() {
  const [armed, setArmed] = useState<string | null>(null);
  const timer = useRef(0);
  const ask = (key: string, run: () => void) => {
    if (armed === key) {
      setArmed(null);
      clearTimeout(timer.current);
      run();
      return;
    }
    setArmed(key);
    clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setArmed(null), 3000);
  };
  return { armed, ask };
}

export function DataSection({ backend }: { backend: Backend }) {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const notice = useUi((s) => s.showNotice);
  const [ov, setOv] = useState<DataOverview | null>(null);
  const [withHistory, setWithHistory] = useState(true);
  const [exported, setExported] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const { armed, ask } = useConfirm();

  const refresh = useCallback(() => {
    backend.dataOverview().then(setOv, () => setOv(null));
  }, [backend]);
  useEffect(refresh, [refresh]);

  /** 后端换了设置（恢复、导入、重置）：界面直接用新的那份 */
  const applySettings = (v: unknown) => useSettings.getState().setAll(mergeSettings(v));

  const run = async (key: string, f: () => Promise<void>) => {
    setBusy(key);
    try {
      await f();
    } catch (e) {
      notice(String(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const onImport = async (file: File) => {
    await run("import", async () => {
      const buf = new Uint8Array(await file.arrayBuffer());
      const r = await backend.dataImport(bytesToBase64(buf), withHistory);
      if (r.settings) applySettings(r.settings);
      const parts = [r.settings ? "设置" : null, r.backgrounds ? `${r.backgrounds} 张背景图` : null, r.historyAdded !== null ? `${r.historyAdded} 条命令历史` : null].filter(Boolean);
      notice(parts.length ? `已导入：${parts.join("、")}` : "备份里没有可导入的内容");
    });
  };

  const total = ov ? ov.settingsBytes + ov.backgrounds.reduce((a, b) => a + b.bytes, 0) + ov.backupsBytes + ov.historyBytes : 0;
  const bgInUse = settings.background.image;

  return (
    <>
      <Group title="保存在哪里">
        <div className="set-hint">所有数据都只保存在这台电脑上{ov ? `，一共占用 ${formatBytes(total)}` : ""}。</div>
        <Row label="设置、背景图、设置备份" desc={<span className="mono-path">{ov?.configDir}</span>}>
          <button type="button" className="btn" onClick={() => backend.dataReveal("config").catch((e) => notice(String(e)))}>
            打开文件夹
          </button>
        </Row>
        <Row label="命令历史" desc={<span className="mono-path">{ov?.dataDir}</span>}>
          <button type="button" className="btn" onClick={() => backend.dataReveal("data").catch((e) => notice(String(e)))}>
            打开文件夹
          </button>
        </Row>
      </Group>

      <Group
        title="设置备份"
        action={
          <button type="button" className="link-btn" disabled={busy === "backup"} onClick={() =>
            run("backup", async () => {
              const name = await backend.dataBackupNow();
              notice(name ? "已备份当前设置" : "还没改过设置，不用备份");
            })
          }>
            立即备份
          </button>
        }
      >
        <div className="set-hint">每天自动备份一次；导入、恢复、重置之前也会先备份当前的设置。最多保留 20 份。</div>
        {ov && ov.backups.length === 0 && <div className="set-hint">还没有备份（还没改过任何设置）。</div>}
        <ul className="data-list">
          {ov?.backups.map((b) => (
            <li key={b.name}>
              <span className="dl-main">{fullTime(b.at)}</span>
              <span className="dl-tag">{REASONS[b.reason] ?? b.reason}</span>
              <span className="dl-size">{formatBytes(b.bytes)}</span>
              <button
                type="button"
                className={`btn small${armed === b.name ? " danger" : ""}`}
                onClick={() =>
                  ask(b.name, () =>
                    run("restore", async () => {
                      applySettings(await backend.dataRestoreBackup(b.name));
                      notice("已恢复这份设置");
                    }),
                  )
                }
              >
                {armed === b.name ? "再点一次恢复" : "恢复"}
              </button>
            </li>
          ))}
        </ul>
      </Group>

      <Group title="导出与导入">
        <div className="set-hint">把设置、导入的背景图（可选：命令历史）打成一个 .zip 文件，换电脑或重装系统后导入就能恢复。</div>
        <Row label="包含命令历史" desc={ov?.historyCount != null ? `${ov.historyCount} 条，${formatBytes(ov.historyBytes)}。导入时和现有的历史合并，不会重复` : undefined}>
          <Toggle label="包含命令历史" value={withHistory} onChange={setWithHistory} />
        </Row>
        <Row label="导出备份" desc={exported ? <span className="mono-path">已保存到 {exported}</span> : `保存到 ${ov?.exportDir ?? "下载文件夹"}`}>
          <div className="key-row">
            {exported && (
              <button type="button" className="link-btn" onClick={() => backend.dataReveal(exported).catch((e) => notice(String(e)))}>
                在文件夹中显示
              </button>
            )}
            <button
              type="button"
              className="btn primary"
              disabled={busy === "export"}
              onClick={() =>
                run("export", async () => {
                  setExported(await backend.dataExport(withHistory));
                  notice("备份已导出");
                })
              }
            >
              {busy === "export" ? "正在导出…" : "导出"}
            </button>
          </div>
        </Row>
        <Row label="导入备份" desc="当前的设置会先自动备份，导入后可以在上面恢复回来">
          <input
            ref={fileInput}
            type="file"
            accept=".zip"
            hidden
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (f) onImport(f);
            }}
          />
          <button type="button" className="btn" disabled={busy === "import"} onClick={() => fileInput.current?.click()}>
            {busy === "import" ? "正在导入…" : "选择备份文件…"}
          </button>
        </Row>
      </Group>

      <Group title="背景图">
        {ov && ov.backgrounds.length === 0 && <div className="set-hint">没有导入过背景图（内置的四张不占空间）。</div>}
        <ul className="data-list">
          {ov?.backgrounds.map((b) => (
            <li key={b.name}>
              <span className="dl-main" title={b.name}>
                {b.name}
              </span>
              {bgInUse === b.name ? <span className="dl-tag on">正在使用</span> : <span />}
              <span className="dl-size">{formatBytes(b.bytes)}</span>
              <button
                type="button"
                className={`btn small${armed === `bg-${b.name}` ? " danger" : ""}`}
                onClick={() =>
                  ask(`bg-${b.name}`, () =>
                    run("bg", async () => {
                      await backend.backgroundDelete(b.name);
                      if (useSettings.getState().settings.background.image === b.name) update((d) => (d.background.image = null));
                    }),
                  )
                }
              >
                {armed === `bg-${b.name}` ? "再点一次删除" : "删除"}
              </button>
            </li>
          ))}
        </ul>
      </Group>

      <Group title="命令历史">
        <Row label="保留多久" desc="更早的记录在每次启动时自动清理，收藏的命令不受影响">
          <Select label="保留多久" value={settings.history.keepDays} options={KEEP_OPTIONS} onChange={(v) => update((d) => (d.history.keepDays = v))} />
        </Row>
        {settings.history.keepDays > 0 && (
          <Row label={`现在就清理 ${settings.history.keepDays} 天前的记录`}>
            <button
              type="button"
              className="btn"
              onClick={() => run("prune", async () => notice(`删掉了 ${await backend.historyPrune(settings.history.keepDays)} 条`))}
            >
              清理
            </button>
          </Row>
        )}
        <Row label="清空全部命令历史" desc="不能撤销。想留一份的话，先在上面导出备份">
          <button
            type="button"
            className={`btn${armed === "clear" ? " danger" : ""}`}
            onClick={() => ask("clear", () => run("clear", async () => notice(`删掉了 ${await backend.historyPrune(0)} 条`)))}
          >
            {armed === "clear" ? "再点一次清空" : "清空"}
          </button>
        </Row>
      </Group>

      <Group title="恢复默认">
        <Row label="把所有设置恢复成默认值" desc="当前的设置会先自动备份，可以在上面的“设置备份”里恢复回来。背景图和命令历史不受影响">
          <button
            type="button"
            className={`btn${armed === "reset" ? " danger" : ""}`}
            onClick={() =>
              ask("reset", () =>
                run("reset", async () => {
                  applySettings(await backend.settingsReset());
                  notice("已恢复默认设置");
                }),
              )
            }
          >
            {armed === "reset" ? "再点一次恢复默认" : "恢复默认"}
          </button>
        </Row>
      </Group>
    </>
  );
}
