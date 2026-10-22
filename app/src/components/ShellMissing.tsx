// 找不到 PowerShell 7 时，标签页里显示的说明和办法（docs/02 §3.1）。
// 不崩溃、也不悄悄换成旧版：告诉用户缺了什么，给出几条路，选哪条由用户决定。
import { useEffect, useState } from "react";
import type { Backend, ShellStatus } from "../backend/types";
import { newTabId, useStore } from "../state/store";
import { useUi } from "../state/ui";
import { Icon } from "./Icons";

const DOWNLOAD_URL = "https://aka.ms/PSWindows";

export function ShellMissing({ tabId, backend }: { tabId: string; backend: Backend }) {
  const [status, setStatus] = useState<ShellStatus | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    backend.shellStatus().then(setStatus, () => setStatus(null));
  }, [backend]);

  /** 把这个标签页换成另一种：关掉自己，在原位置开新的 */
  const replaceWith = (launch?: "windowsPowerShell" | "installPwsh") => {
    const s = useStore.getState();
    const cwd = s.tabs.find((t) => t.id === tabId)?.initialCwd;
    const profile = launch === "installPwsh" ? "安装 PowerShell 7" : launch === "windowsPowerShell" ? "Windows PowerShell" : undefined;
    s.addTab(newTabId(), { cwd, launch, profile });
    s.closeTab(tabId);
  };

  const recheck = async () => {
    setChecking(true);
    const st = await backend.shellStatus().catch(() => null);
    setChecking(false);
    setStatus(st);
    if (st?.pwsh) replaceWith();
    else useUi.getState().showNotice("还是没有找到 PowerShell 7");
  };

  return (
    <div className="shell-missing" role="alert">
      <div className="sm-card">
        <div className="sm-head">
          <span className="sm-icon">
            <Icon name="pwsh" size={22} />
          </span>
          <div>
            <h2>这台电脑上还没有 PowerShell 7</h2>
            <p>
              mooncli 基于 PowerShell 7（微软出品，免费）。Windows 自带的是旧版 Windows PowerShell 5.1，很多智能功能用不了。
              {status?.configuredMissing && " 设置里指定的 pwsh.exe 位置也找不到文件。"}
            </p>
          </div>
        </div>
        <div className="sm-options">
          {status?.winget !== false && (
            <button type="button" className="sm-option primary" onClick={() => replaceWith("installPwsh")}>
              <strong>一键安装（推荐）</strong>
              <span>用 Windows 自带的 winget 从微软官方源安装，过程在这个标签页里显示，装好后自动打开</span>
            </button>
          )}
          <button type="button" className="sm-option" onClick={() => backend.openUrl(DOWNLOAD_URL).catch(() => useUi.getState().showNotice(DOWNLOAD_URL))}>
            <strong>去微软官网下载</strong>
            <span>{status?.winget === false ? "这台电脑没有 winget，" : ""}下载 .msi 安装包自己装，装好后点下面的"重新检测"</span>
          </button>
          {status?.windowsPowershell && (
            <button type="button" className="sm-option" onClick={() => replaceWith("windowsPowerShell")}>
              <strong>先用 Windows PowerShell 5.1</strong>
              <span>马上就能用，但命令记录、配色适配等功能可能不完整</span>
            </button>
          )}
          <button type="button" className="sm-option" onClick={() => useUi.getState().openSettings("shell")}>
            <strong>已经装了，指定位置</strong>
            <span>装在了不常见的地方时，在设置 → Shell 里填 pwsh.exe 的路径</span>
          </button>
        </div>
        <div className="sm-foot">
          <button type="button" className="btn" disabled={checking} onClick={recheck}>
            {checking ? "正在检测…" : "重新检测"}
          </button>
        </div>
      </div>
    </div>
  );
}
