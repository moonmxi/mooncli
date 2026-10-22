// 命令说明书（docs/04 §4 的第三个来源）：常用外部程序的子命令和参数，带中文说明。
// 结构参考 Fig autocomplete specs（MIT），内容按日常最常用的部分自己整理，欢迎补充。
// 只放"不用跑程序就知道"的静态信息；分支名、包名这类动态内容交给 pwsh 补全和历史。

export interface SpecOption {
  /** 写法，第一个是主名：["-m", "--message"] */
  names: string[];
  desc: string;
}

export interface SpecCommand {
  name: string;
  desc: string;
  subcommands?: SpecCommand[];
  options?: SpecOption[];
}

const o = (names: string, desc: string): SpecOption => ({ names: names.split(/,\s*/), desc });
const c = (name: string, desc: string, options?: SpecOption[], subcommands?: SpecCommand[]): SpecCommand => ({ name, desc, options, subcommands });

const HELP = o("-h, --help", "显示帮助");

export const SPECS: SpecCommand[] = [
  c("git", "分布式版本控制", [o("-C", "在指定目录里运行"), o("--version", "显示版本")], [
    c("status", "查看工作区状态", [o("-s, --short", "简短格式"), o("-b, --branch", "显示分支信息")]),
    c("add", "把改动加入暂存区", [o("-A, --all", "加入全部改动"), o("-p, --patch", "逐块挑选"), o("-u, --update", "只加已跟踪的文件")]),
    c("commit", "提交暂存区的改动", [o("-m, --message", "提交说明"), o("-a, --all", "自动暂存已跟踪文件的改动"), o("--amend", "修改上一次提交"), o("--no-edit", "沿用原来的提交说明"), o("-S", "用 GPG 签名")]),
    c("push", "推送到远程", [o("-u, --set-upstream", "设置上游分支"), o("--force-with-lease", "安全的强制推送"), o("-f, --force", "强制推送"), o("--tags", "推送标签")]),
    c("pull", "拉取并合并", [o("--rebase", "用变基代替合并"), o("--ff-only", "只允许快进")]),
    c("fetch", "获取远程更新", [o("--all", "所有远程"), o("-p, --prune", "清掉远程已删除的分支")]),
    c("checkout", "切换分支或恢复文件", [o("-b", "新建并切换到分支"), o("--", "后面是文件路径")]),
    c("switch", "切换分支", [o("-c, --create", "新建并切换"), o("-", "切回上一个分支")]),
    c("branch", "列出、新建、删除分支", [o("-a, --all", "包括远程分支"), o("-d, --delete", "删除分支"), o("-D", "强制删除分支"), o("-m, --move", "重命名分支"), o("-v", "显示最后一次提交")]),
    c("merge", "合并分支", [o("--no-ff", "总是生成合并提交"), o("--squash", "压成一个改动"), o("--abort", "放弃合并")]),
    c("rebase", "变基", [o("-i, --interactive", "交互式变基"), o("--continue", "解决冲突后继续"), o("--abort", "放弃变基"), o("--onto", "变基到指定提交")]),
    c("log", "查看提交历史", [o("--oneline", "每条一行"), o("--graph", "画出分支图"), o("-n", "只看最近 n 条"), o("-p", "显示改动内容"), o("--stat", "显示改动统计")]),
    c("diff", "查看改动", [o("--staged, --cached", "看暂存区的改动"), o("--stat", "只看统计"), o("--name-only", "只列文件名")]),
    c("stash", "暂存手头的改动", [o("-u, --include-untracked", "包括未跟踪文件")], [
      c("push", "暂存改动", [o("-m, --message", "说明")]),
      c("pop", "取回并删除最近一次暂存"),
      c("list", "列出暂存"),
      c("apply", "取回但不删除"),
      c("drop", "删除一条暂存"),
    ]),
    c("reset", "撤销提交或暂存", [o("--soft", "只移动 HEAD"), o("--mixed", "同时撤销暂存（默认）"), o("--hard", "丢弃全部改动")]),
    c("restore", "恢复文件", [o("--staged", "撤销暂存"), o("-s, --source", "从指定提交恢复")]),
    c("clone", "克隆仓库", [o("--depth", "浅克隆的深度"), o("-b, --branch", "克隆后切到的分支"), o("--recurse-submodules", "连子模块一起")]),
    c("remote", "管理远程仓库", [o("-v", "显示地址")], [c("add", "添加远程"), c("remove", "删除远程"), c("set-url", "修改地址")]),
    c("tag", "管理标签", [o("-a", "带说明的标签"), o("-d", "删除标签"), o("-l", "列出标签")]),
    c("cherry-pick", "把某个提交应用到当前分支", [o("--continue", "继续"), o("--abort", "放弃")]),
    c("show", "查看某个提交"),
    c("blame", "查看每行最后是谁改的"),
    c("clean", "删除未跟踪的文件", [o("-n, --dry-run", "只看会删什么"), o("-f, --force", "真的删除"), o("-d", "包括目录")]),
    c("worktree", "管理多个工作区", undefined, [c("add", "新建工作区"), c("list", "列出工作区"), c("remove", "删除工作区")]),
    c("init", "新建仓库"),
  ]),
  c("gh", "GitHub 命令行", [HELP], [
    c("pr", "拉取请求", undefined, [c("create", "新建 PR"), c("list", "列出 PR"), c("view", "查看 PR"), c("checkout", "检出 PR"), c("merge", "合并 PR"), c("status", "我相关的 PR")]),
    c("issue", "问题", undefined, [c("create", "新建问题"), c("list", "列出问题"), c("view", "查看问题")]),
    c("repo", "仓库", undefined, [c("clone", "克隆"), c("create", "新建"), c("view", "查看")]),
    c("release", "发布", undefined, [c("create", "新建发布"), c("list", "列出发布"), c("upload", "上传文件"), c("view", "查看发布")]),
    c("run", "Actions 运行记录", undefined, [c("list", "列出运行"), c("view", "查看运行"), c("watch", "盯着一次运行")]),
    c("auth", "登录", undefined, [c("login", "登录"), c("status", "登录状态")]),
  ]),
  c("npm", "Node 包管理器", [HELP], [
    c("install", "安装依赖", [o("-D, --save-dev", "装成开发依赖"), o("-g, --global", "全局安装"), o("--save-exact", "锁定精确版本")]),
    c("run", "运行 package.json 里的脚本"),
    c("test", "运行测试"),
    c("start", "运行 start 脚本"),
    c("uninstall", "卸载包", [o("-g, --global", "全局")]),
    c("update", "升级依赖"),
    c("outdated", "列出可升级的依赖"),
    c("init", "新建 package.json", [o("-y, --yes", "全部用默认值")]),
    c("publish", "发布包", [o("--access", "public 或 restricted"), o("--dry-run", "只演练")]),
    c("ci", "按锁文件干净安装"),
    c("ls", "列出已装的包", [o("--depth", "显示的层数")]),
  ]),
  c("npx", "临时运行一个 npm 包里的命令", [o("-y, --yes", "需要安装时不再询问")]),
  c("pnpm", "快速、省空间的 Node 包管理器", [HELP], [
    c("install", "安装依赖", [o("--frozen-lockfile", "不修改锁文件")]),
    c("add", "添加依赖", [o("-D, --save-dev", "开发依赖"), o("-g, --global", "全局")]),
    c("remove", "删除依赖"),
    c("run", "运行脚本"),
    c("dev", "运行 dev 脚本"),
    c("build", "运行 build 脚本"),
    c("test", "运行测试"),
    c("update", "升级依赖", [o("-i, --interactive", "交互式选择"), o("--latest", "升到最新版本")]),
    c("dlx", "临时运行一个包"),
    c("exec", "在项目环境里运行命令"),
  ]),
  c("yarn", "Node 包管理器", [HELP], [c("add", "添加依赖", [o("-D, --dev", "开发依赖")]), c("remove", "删除依赖"), c("install", "安装依赖"), c("run", "运行脚本"), c("upgrade", "升级依赖")]),
  c("node", "运行 JavaScript", [o("-e, --eval", "执行一段代码"), o("--inspect", "开启调试"), o("--watch", "文件变了自动重启"), o("-v, --version", "显示版本")]),
  c("cargo", "Rust 构建工具", [HELP, o("-V, --version", "显示版本")], [
    c("build", "编译", [o("-r, --release", "发布模式"), o("-p, --package", "只编译某个包"), o("--target", "目标平台")]),
    c("run", "编译并运行", [o("-r, --release", "发布模式"), o("--bin", "运行哪个程序"), o("--example", "运行示例")]),
    c("test", "运行测试", [o("-p, --package", "只测某个包"), o("--release", "发布模式"), o("--", "后面的参数传给测试程序")]),
    c("check", "检查能否编译（不生成文件）", [o("--all-targets", "包括测试和示例")]),
    c("clippy", "代码检查", [o("--all-targets", "包括测试和示例"), o("--fix", "自动修复")]),
    c("fmt", "格式化代码", [o("--check", "只检查不修改"), o("--all", "整个工作区")]),
    c("add", "添加依赖", [o("--dev", "开发依赖"), o("-F, --features", "启用特性")]),
    c("new", "新建项目", [o("--lib", "库项目")]),
    c("update", "更新锁文件"),
    c("doc", "生成文档", [o("--open", "生成后打开")]),
    c("install", "安装 Rust 程序", [o("--locked", "使用锁文件")]),
    c("clean", "删除编译产物"),
    c("bench", "运行基准测试"),
    c("publish", "发布到 crates.io", [o("--dry-run", "只演练")]),
  ]),
  c("rustup", "Rust 工具链管理", undefined, [c("update", "升级工具链"), c("default", "设置默认工具链"), c("target", "管理编译目标", undefined, [c("add", "添加目标"), c("list", "列出目标")]), c("component", "管理组件", undefined, [c("add", "添加组件")]), c("show", "显示当前工具链")]),
  c("python", "运行 Python", [o("-m", "以模块方式运行"), o("-c", "执行一段代码"), o("-V, --version", "显示版本")]),
  c("pip", "Python 包管理器", undefined, [
    c("install", "安装包", [o("-r, --requirement", "从需求文件安装"), o("-U, --upgrade", "升级"), o("-e, --editable", "可编辑安装"), o("--user", "装到用户目录")]),
    c("uninstall", "卸载包", [o("-y, --yes", "不再确认")]),
    c("list", "列出已装的包", [o("-o, --outdated", "只看可升级的")]),
    c("freeze", "输出需求文件格式的列表"),
    c("show", "查看包的信息"),
  ]),
  c("uv", "快速的 Python 包与项目管理", undefined, [c("run", "在项目环境里运行"), c("add", "添加依赖"), c("remove", "删除依赖"), c("sync", "按锁文件同步环境"), c("venv", "新建虚拟环境"), c("pip", "兼容 pip 的命令"), c("init", "新建项目")]),
  c("docker", "容器", [HELP], [
    c("run", "运行容器", [o("-d, --detach", "后台运行"), o("-it", "交互式终端"), o("-p, --publish", "端口映射 主机:容器"), o("-v, --volume", "挂载目录"), o("-e, --env", "设置环境变量"), o("--name", "容器名"), o("--rm", "退出后删除"), o("--network", "网络")]),
    c("ps", "列出容器", [o("-a, --all", "包括已停止的")]),
    c("images", "列出镜像"),
    c("build", "构建镜像", [o("-t, --tag", "镜像名:标签"), o("-f, --file", "Dockerfile 路径"), o("--no-cache", "不用缓存")]),
    c("exec", "在运行中的容器里执行命令", [o("-it", "交互式终端")]),
    c("logs", "查看日志", [o("-f, --follow", "持续输出"), o("--tail", "只看最后几行")]),
    c("stop", "停止容器"),
    c("rm", "删除容器", [o("-f, --force", "强制")]),
    c("rmi", "删除镜像"),
    c("pull", "拉取镜像"),
    c("push", "推送镜像"),
    c("compose", "多容器编排", undefined, [c("up", "启动", [o("-d", "后台运行"), o("--build", "先构建")]), c("down", "停止并删除"), c("logs", "查看日志", [o("-f", "持续输出")]), c("ps", "列出服务"), c("restart", "重启服务")]),
    c("system", "系统", undefined, [c("prune", "清理没用的数据", [o("-a", "包括没用到的镜像")]), c("df", "磁盘占用")]),
  ]),
  c("kubectl", "Kubernetes 命令行", [o("-n, --namespace", "命名空间"), o("--context", "使用哪个上下文")], [
    c("get", "列出资源", [o("-o, --output", "输出格式 wide/yaml/json"), o("-A, --all-namespaces", "所有命名空间"), o("-w, --watch", "持续观察")]),
    c("describe", "查看资源详情"),
    c("apply", "按文件创建或更新", [o("-f, --filename", "文件或目录")]),
    c("delete", "删除资源", [o("-f, --filename", "文件")]),
    c("logs", "查看日志", [o("-f, --follow", "持续输出"), o("--tail", "只看最后几行"), o("-c, --container", "容器名")]),
    c("exec", "在容器里执行命令", [o("-it", "交互式终端")]),
    c("port-forward", "端口转发"),
    c("rollout", "发布管理", undefined, [c("status", "发布状态"), c("restart", "滚动重启"), c("undo", "回滚")]),
    c("config", "配置", undefined, [c("use-context", "切换上下文"), c("get-contexts", "列出上下文")]),
  ]),
  c("winget", "Windows 软件包管理", undefined, [
    c("install", "安装软件", [o("--id", "按软件 ID"), o("-e, --exact", "精确匹配"), o("--accept-package-agreements", "接受许可协议")]),
    c("upgrade", "升级软件", [o("--all", "全部升级")]),
    c("search", "搜索软件"),
    c("uninstall", "卸载软件"),
    c("list", "列出已装的软件"),
    c("show", "查看软件信息"),
  ]),
  c("scoop", "Windows 命令行软件包管理", undefined, [c("install", "安装"), c("update", "更新", [o("*", "全部更新")]), c("uninstall", "卸载"), c("search", "搜索"), c("list", "列出已装的"), c("bucket", "管理软件源", undefined, [c("add", "添加软件源")])]),
  c("dotnet", ".NET 命令行", undefined, [c("build", "编译"), c("run", "运行"), c("test", "测试"), c("new", "新建项目"), c("add", "添加引用", undefined, [c("package", "添加 NuGet 包")]), c("restore", "还原依赖"), c("publish", "发布", [o("-c, --configuration", "Debug 或 Release")])]),
  c("go", "Go 工具链", undefined, [c("build", "编译"), c("run", "编译并运行"), c("test", "测试", [o("-v", "详细输出"), o("-run", "只跑匹配的测试")]), c("mod", "模块", undefined, [c("tidy", "整理依赖"), c("init", "新建模块")]), c("get", "获取依赖"), c("fmt", "格式化")]),
  c("ssh", "远程登录", [o("-p", "端口"), o("-i", "私钥文件"), o("-L", "本地端口转发"), o("-R", "远程端口转发"), o("-J", "跳板机"), o("-v", "显示调试信息")]),
  c("scp", "通过 ssh 复制文件", [o("-r", "复制目录"), o("-P", "端口"), o("-i", "私钥文件")]),
  c("wsl", "Linux 子系统", [o("-d, --distribution", "用哪个发行版"), o("-l, --list", "列出发行版"), o("--shutdown", "关闭全部发行版"), o("-u, --user", "以哪个用户运行"), o("--install", "安装")]),
  c("code", "VS Code", [o("-n, --new-window", "新窗口"), o("-r, --reuse-window", "用已打开的窗口"), o("-d, --diff", "比较两个文件"), o("-g, --goto", "打开到指定行")]),
  c("claude", "Claude Code", [o("-p, --print", "非交互：输出结果后退出"), o("-c, --continue", "接着上一次的会话"), o("-r, --resume", "恢复某个会话"), o("--model", "使用哪个模型"), o("--settings", "额外的设置文件")], [
    c("mcp", "管理 MCP 服务器"),
    c("config", "查看或修改设置"),
    c("update", "升级 Claude Code"),
    c("doctor", "检查安装是否正常"),
  ]),
  c("codex", "OpenAI Codex", [o("-m, --model", "使用哪个模型"), o("-C, --cd", "工作目录"), o("-c, --config", "临时覆盖设置")], [
    c("exec", "非交互运行", [o("--skip-git-repo-check", "不检查是否在 git 仓库里"), o("--json", "输出 JSON 事件")]),
    c("resume", "恢复会话", [o("--last", "最近一次")]),
    c("login", "登录"),
    c("mcp", "管理 MCP 服务器"),
    c("app-server", "运行 app-server（IDE 插件用）"),
  ]),
  c("Get-ChildItem", "列出目录内容（ls / dir）", [o("-Recurse", "包括子目录"), o("-Force", "包括隐藏文件"), o("-Filter", "按名字筛选"), o("-File", "只要文件"), o("-Directory", "只要文件夹")]),
  c("Set-Location", "切换目录（cd）", [o("-Path", "目标路径")]),
  c("Remove-Item", "删除（rm / del）", [o("-Recurse", "包括子目录"), o("-Force", "强制，包括只读和隐藏的"), o("-WhatIf", "只看会删什么")]),
  c("Copy-Item", "复制（cp）", [o("-Recurse", "包括子目录"), o("-Destination", "目标"), o("-Force", "覆盖")]),
  c("Move-Item", "移动或重命名（mv）", [o("-Destination", "目标"), o("-Force", "覆盖")]),
  c("Get-Content", "读文件（cat）", [o("-Tail", "只看最后几行"), o("-Wait", "持续输出新内容"), o("-Encoding", "编码"), o("-Raw", "整个文件当一个字符串")]),
  c("Select-String", "按正则查找文字（grep）", [o("-Pattern", "要找的内容"), o("-Path", "在哪些文件里找"), o("-CaseSensitive", "区分大小写"), o("-Context", "带上前后几行")]),
];

const byName = new Map(SPECS.map((s) => [s.name.toLowerCase(), s]));
// 常用别名指向同一份说明
for (const [alias, target] of [
  ["ls", "Get-ChildItem"],
  ["dir", "Get-ChildItem"],
  ["gci", "Get-ChildItem"],
  ["cd", "Set-Location"],
  ["rm", "Remove-Item"],
  ["del", "Remove-Item"],
  ["cp", "Copy-Item"],
  ["mv", "Move-Item"],
  ["cat", "Get-Content"],
  ["sls", "Select-String"],
  ["py", "python"],
  ["pip3", "pip"],
  ["python3", "python"],
]) {
  const s = byName.get(target.toLowerCase());
  if (s) byName.set(alias, s);
}

export function findSpec(name: string): SpecCommand | undefined {
  return byName.get(name.toLowerCase().replace(/\.(exe|cmd|ps1)$/, ""));
}

export interface SpecCandidate {
  /** 要填进去的文字 */
  text: string;
  desc: string;
  kind: "command" | "subcommand" | "option";
}

/**
 * 按光标前的输入算候选：
 *   `gi`            → 命令名（git、gh……）
 *   `git ch`        → 子命令（checkout、cherry-pick……）
 *   `git commit --a`→ 参数（--amend、--all）
 * 返回的候选都替换"光标前的当前词"。
 */
export function specCandidates(beforeCursor: string, limit = 30): SpecCandidate[] {
  const tokens = beforeCursor.split(/\s+/);
  const word = tokens[tokens.length - 1] ?? "";
  const done = tokens.slice(0, -1).filter(Boolean);
  const lw = word.toLowerCase();
  if (done.length === 0) {
    if (!word) return [];
    return SPECS.filter((s) => s.name.toLowerCase().startsWith(lw) && s.name.toLowerCase() !== lw)
      .slice(0, limit)
      .map((s) => ({ text: s.name, desc: s.desc, kind: "command" }));
  }
  const root = findSpec(done[0]);
  if (!root) return [];
  let cur: SpecCommand = root;
  // 沿着已经输入的子命令往下走（跳过参数）
  for (const t of done.slice(1)) {
    if (t.startsWith("-")) continue;
    const sub: SpecCommand | undefined = cur.subcommands?.find((s) => s.name === t);
    if (!sub) break;
    cur = sub;
  }
  const out: SpecCandidate[] = [];
  if (!word.startsWith("-")) {
    for (const s of cur.subcommands ?? []) if (s.name.startsWith(word) && s.name !== word) out.push({ text: s.name, desc: s.desc, kind: "subcommand" });
  }
  if (word.startsWith("-") || out.length === 0) {
    for (const op of cur.options ?? []) {
      const hit = op.names.find((n) => n.toLowerCase().startsWith(lw) && n !== word);
      if (hit && (word || op.names[0].startsWith("-"))) out.push({ text: hit, desc: op.desc, kind: "option" });
    }
  }
  return out.slice(0, limit);
}
