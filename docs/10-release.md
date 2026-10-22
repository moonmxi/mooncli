# 10 发布与签名

## 1. 去哪下载

| 版本 | 怎么来的 | 地址 | 签名 |
| --- | --- | --- | --- |
| 预览版（nightly） | 每次推送 main 自动构建，替换成最新的一份（其它分支只构建、测试，不发布） | `https://github.com/moonmxi/mooncli/releases/download/nightly/mooncli-setup-x64.exe` | 不签 |
| 正式版本 | 推 `v0.2.0` 这样的标签时构建 | [Releases 页面](https://github.com/moonmxi/mooncli/releases) | 暂未签名（SignPath 申请未通过，见 §3） |

两种都是直接发到 GitHub 的 **Release 页**的 `.exe`，不经过 Actions 的"产物"（那里的文件一定会被打成 zip）。

安装包里附带了新版 ConPTY（`conpty.dll`、`OpenConsole.exe`，来自微软官方的 NuGet 包，MIT 协议），由 `tools/fetch_conpty.py` 在打包前下载并校验哈希。安装包和安装向导用的是 mooncli 的图标（`app/src-tauri/icons/icon.ico`、`nsis-header.bmp`、`nsis-sidebar.bmp`）。

## 2. 为什么安装时会被拦

下载和安装没有签名的程序时，Windows 会出两道提醒：

| 提醒 | 谁发的 | 原因 |
| --- | --- | --- |
| 浏览器提示"不常下载，可能有害" | Edge / Chrome 的下载保护 | 文件没有签名，下载的人又少 |
| "Windows 已保护你的电脑" | Windows SmartScreen | 同上 |

这两道提醒看的是**发布者是谁、有没有信誉**，不看程序内容，所以改代码去不掉，只能签名。mooncli 是开源项目，原计划用 **SignPath Foundation 的免费签名**（基金会以自己的名义持有证书，给符合条件的开源项目签名，不收费）。2026-10-22 申请未获批准：项目刚发布，缺少对方要求的公开知名度信号。攒够信号后重新申请（见 §3）；在此之前，正式版本和预览版一样不签名。

不会做的：教用户关掉 SmartScreen，或者用技术手段绕过提醒。

## 3. 申请 SignPath 免费签名

### 已经准备好的

SignPath 的条件（[signpath.org/terms](https://signpath.org/terms)）和仓库里对应的准备：

| 条件 | 怎么满足 |
| --- | --- |
| 开源协议（OSI 认可），所有组件都开源 | [`LICENSE`](../LICENSE) 是 MIT；附带的 Nerd Font 符号字体和 ConPTY 也是 MIT |
| 项目在维护、已经发布过 | 仓库公开，Release 页有 `nightly` 和 `v0.1.0` 正式版本 |
| 主页和下载页写明"代码签名政策" | 2026-10-22 决定暂不展示（申请未通过，README 已移除该节）；重新申请前放回 README，内容含致谢语、团队角色、批准方式（旧版可从 git 历史找回） |
| 隐私说明 | README 的"隐私"一节，用的是 SignPath 建议的说法，另外说明了 WebView2 的下载 |
| 只签自己源码构建的程序，在可信的构建系统上构建 | CI 在 GitHub Actions 上构建，安装包直接从构建任务交给 SignPath |
| 每次签名都要人工批准 | SignPath 那边设置"批准人"；所以只给正式版本签名，nightly 不签 |
| 产品名、版本号要和项目一致 | 安装包的产品名是 mooncli；CI 会检查标签和 `tauri.conf.json` 里的版本号一致 |
| 安装程序要说明对系统做了什么、能卸载 | Tauri 生成的 NSIS 安装程序装在当前用户目录，带卸载程序 |
| 团队成员都开启两步验证 | 已开启（2026-10 确认） |

### 申请结果（2026-10-22）

- 2026-10-22 提交申请，当天收到结果：**未获批准**。理由是项目刚发布，缺少 SignPath 要求的公开知名度信号（GitHub stars / forks / 贡献者、外部文章与讨论、持续活跃度等），与代码质量无关。
- 对方明确欢迎攒够信号后重新申请；也可以改用付费订阅，当前不考虑。
- 当前决定：先不签名发布，把精力放在产品和传播上——stars 和外部讨论正是重申时要看的信号。

### 重新申请通过后要做的

1. **在 SignPath 网页上建项目**（通过后按他们的引导做）：
   - 关联 GitHub 仓库，把 GitHub Actions 设为可信的构建系统；
   - 新建制品配置（Artifact Configuration），内容直接用仓库里的 [`.signpath/artifact-configuration.xml`](../.signpath/artifact-configuration.xml)；
   - 新建签名策略（Signing Policy），比如叫 `release-signing`，批准人设成自己；
   - 生成一个 API Token。
2. **把这些填进 GitHub 仓库**（Settings → Secrets and variables → Actions）：

   | 类型 | 名称 | 内容 |
   | --- | --- | --- |
   | Secret | `SIGNPATH_API_TOKEN` | 上一步生成的 API Token |
   | Variable | `SIGNPATH_ORGANIZATION_ID` | SignPath 组织 ID |
   | Variable | `SIGNPATH_PROJECT_SLUG` | 项目的短名，比如 `mooncli` |
   | Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | 签名策略的短名，比如 `release-signing` |
   | Variable | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | 可选，不填就用项目的默认制品配置 |

3. 之后每次发版本：推 `v*` 标签（或在网页上新建发布） → CI 构建并把安装包交给 SignPath → 你在 SignPath 网页（或邮件链接）上点"批准" → CI 拿回签好的安装包发到 Release 页。CI 最多等一小时。

### 签名之后还会提醒吗

签名后不再显示"未知发布者"，发布者显示为 SignPath Foundation。SmartScreen 的信誉还要随着下载量积累，最开始几个版本仍可能偶尔提醒，之后会消失。只给安装包签名（程序本体在安装包里是压缩的，没法单独签），SmartScreen 检查的就是下载下来的安装包。

## 4. 其它签名方式（备用）

CI 里还保留了 **Azure Artifact Signing**（原 Trusted Signing，微软的付费签名服务，约每月 10 美元）的步骤：配置 `AZURE_CLIENT_ID`、`AZURE_CLIENT_SECRET`、`AZURE_TENANT_ID`、`SIGNING_ENDPOINT`、`SIGNING_ACCOUNT`、`SIGNING_PROFILE` 后，每次构建都会通过 Tauri 的 `bundle.windows.signCommand` 签名。两种方式不要同时配。

| 办法 | 成本 | 说明 |
| --- | --- | --- |
| SignPath Foundation | 免费 | 开源项目，需审核，每次签名人工批准；2026-10-22 首次申请未通过（知名度信号不足），攒信号后重申 |
| Azure Artifact Signing | 约每月 10 美元 | 身份验证目前只对美国、加拿大开放，国内个人开发者无法申请（2026-10 查证） |
| 普通 OV 证书 | 每年几百到上千元 | 信誉要慢慢积累 |
| Microsoft Store | 个人开发者免费注册 **【待核实】** | 要改成 MSIX 格式并通过审核 |

## 5. 已经建好的发布没有安装包怎么办

在网页上新建发布时会同时创建标签，CI 会自动构建并把安装包补进去。如果那次构建失败了（比如 v0.1.0 第一次就因为发布脚本的语法错误没传上去），不用删掉重建：

打开仓库的 **Actions → CI → Run workflow**，分支选 `main`，"发到哪个正式版本"填 `v0.1.0`，点运行。CI 会用 main 上最新的代码构建，把安装包传到这个发布里。版本号必须和 `app/src-tauri/tauri.conf.json` 里的一致。

CI 里内嵌的 PowerShell 脚本由 `tools/check_ps_scripts.py` 在 Linux 上先检查语法。PowerShell 会把中文的弯引号 “ ” 当成字符串的引号，脚本里的中文说明请用「」或直引号。
