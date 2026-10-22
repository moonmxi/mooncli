# 参与开发

欢迎提 issue 和 PR。

## 先跑起来

需要：Rust（stable）、Node.js 22 + pnpm 10、PowerShell 7。

```powershell
cd app
pnpm install
pnpm tauri dev      # 真正的应用（Windows）
pnpm dev            # 只调界面：浏览器打开 http://localhost:1420，用模拟 shell
pnpm test           # 界面逻辑测试
cd ..\ncargo test          # 核心库测试
```

改功能前建议先读 [docs/00 愿景与范围](docs/00-vision.md) 和 [docs/01 总体架构](docs/01-architecture.md)——知道什么不做，和知道做什么一样重要。

## 提 issue

- Bug：用仓库里的 Bug 模板，写清 mooncli 版本、Windows 版本、pwsh 版本和复现步骤；能附截图或日志片段最好。
- 功能建议：先说场景和问题，再说想要的样子。合不合项目范围，看 docs/00。

## 提 PR

- 一件事一个 PR，说明里写清改了什么、为什么。
- 提交前本地跑过 `cargo test` 和 `pnpm test`；CI 红了的 PR 不会合。
- 行为变了，记得把 `docs/` 里对应的文档一起改。
