# 10 发布与签名

## 1. 去哪下载

每次推送，CI 都会在 Windows 上打出安装包，直接发到 GitHub 的 **Release 页**（不是 Actions 的"产物"，那里的文件一定会被打成 zip）：

- 地址固定：`https://github.com/moonmxi/mooncli/releases/download/nightly/mooncli-setup-x64.exe`
- 也可以在仓库的 Releases 页面找到 `nightly` 预发布版，说明里写了是哪个分支、哪次提交打的
- 每次推送都会替换成最新的一份

安装包里附带了新版 ConPTY（`conpty.dll`、`OpenConsole.exe`，来自微软官方的 NuGet 包，MIT 协议），由 `tools/fetch_conpty.py` 在打包前下载并校验哈希。

## 2. 为什么安装时会被拦

下载和安装没有签名的程序时，Windows 会出两道提醒：

| 提醒 | 谁发的 | 原因 |
| --- | --- | --- |
| 浏览器提示"不常下载，可能有害" | Edge / Chrome 的下载保护 | 文件没有签名，下载的人又少 |
| "Windows 已保护你的电脑" | Windows SmartScreen | 同上 |

这两道提醒看的都是**发布者是谁、有没有信誉**，不是看程序内容。所以只靠改代码去不掉，要么给安装包签名，要么换发布渠道。正规的办法有下面几种：

| 办法 | 效果 | 成本 | 需要你做的 |
| --- | --- | --- | --- |
| **Azure Artifact Signing**（原 Trusted Signing） | 微软的签名服务，签出来的程序 SmartScreen 认可度高，通常很快就不再提醒 | 约每月 10 美元；要做身份验证，个人开发者能否申请取决于所在地区 **【待核实】** | 开通服务、完成验证，把密钥填进仓库的 Secrets |
| **SignPath Foundation** | 给开源项目免费签名 | 免费 | 仓库要公开并使用开源协议，提交申请等审核 |
| **普通 OV 代码签名证书** | 签名后不再显示"未知发布者"，但 SmartScreen 信誉要靠下载量慢慢积累，前期可能还会提醒 | 每年几百元到上千元 | 购买证书（现在都放在硬件或云端，不能直接放进 CI，需要配合对应的签名工具） |
| **上架 Microsoft Store** | 由微软签名，从商店安装没有任何提醒 | 个人开发者免费注册 **【待核实】** | 注册开发者账号；我们需要把安装包改成 MSIX 格式并通过审核 |

不推荐、也不会去做的：教用户关掉 SmartScreen，或者用技术手段绕过提醒。这些办法让所有程序都不再受保护，而且不长久。

## 3. CI 里已经准备好的签名步骤

CI 已经写好了 Azure Artifact Signing 的签名步骤，**配好密钥就自动生效**，没配的时候照常打出未签名的安装包。在仓库的 Settings → Secrets and variables → Actions 里添加：

| 名称 | 内容 |
| --- | --- |
| `AZURE_CLIENT_ID`、`AZURE_CLIENT_SECRET`、`AZURE_TENANT_ID` | 有签名权限的 Azure 应用凭据 |
| `SIGNING_ENDPOINT` | 签名账户所在区域的地址，比如 `https://wus2.codesigning.azure.net` |
| `SIGNING_ACCOUNT`、`SIGNING_PROFILE` | 签名账户名和证书配置名 |

配好以后，CI 会安装 `trusted-signing-cli`，并通过 Tauri 的 `bundle.windows.signCommand` 给程序本体和安装包都签上名。签名工具的名字和参数以开通时的官方文档为准，第一次配置后看一眼 CI 日志确认 **【待核实】**。

选其它办法（SignPath、OV 证书）时，只需要把"准备签名工具"那一步换成对应工具的命令。
