# Contributing to Sker

感谢参与 Sker 的开发与治理！Sker 是一个面向高安全性 AI 辅助工程团队的开源项目，请在提交代码前仔细阅读以下规范。

---

## 一、本地开发环境

Sker 遵循 **纯 Rust、零外部 C 依赖** 架构原则，不需要本地安装 OpenSSL、libgit2 或 C 编译链：

- **Rust 工具链**: Rust 1.85+（Edition 2024）
- **依赖检查工具**: `b3sum`（BLAKE3 校验，可选）、`cargo-dist`（发布工作流验证，可选）

```bash
# 克隆仓库
git clone https://github.com/ai-timer/sker.git
cd sker

# 构建工作区全部组件
cargo build --workspace

# 验证无任何 C-linkage 依赖引入
cargo tree -i openssl-sys # 必须输出为空
```

---

## 二、贡献门槛与可选的 SDD 工作流

**普通贡献者无需安装或使用 SDD Kit / `specify-cli`**。修复 Bug、补充规则、完善文档与常规功能改进，直接按下文预提交清单验证后即可发起 PR。

仓库中的 `.specify/` 与 `specs/` 目录仅作为**历史设计档案与宪章（constitution）参考**保留，记录了预览版的设计推演过程；它们不是贡献的前置条件，也不要求随每次 PR 更新。

仅当提议**大型架构特性或 RFC 级变更**时，维护者可能建议采用可选的规范驱动（Spec-Driven Development）工作流，先在 `specs/` 下建立设计目录（`spec.md`、`plan.md`、`tasks.md`）再实现。

无论是否走 SDD 流程，以下工程约束对所有贡献一律生效：

1. **规则防投毒保障**：新增确定性规约（`.sker/rules/` 或中央 Playbook）必须同时包含正例（positive）与反例（negative）fixtures，并通过 `sker compile` 验证；
2. **架构边界不可侵犯**：各 crate 间数据传递必须严格通过 `sker-schema` 进行，不得跨层直连私有结构；
3. **纯 Rust 约束**：不得引入 openssl-sys、libgit2 等 C-linkage 依赖或 `extern "C"`。

---

## 三、强制预提交验证清单

在向 `main` 分支发起 Pull Request 前，请确保以下命令在本地全部通过：

```bash
# 1. 代码格式化校验
cargo fmt --all -- --check

# 2. 运行所有单元测试与集成回归测试
cargo test --workspace

# 3. 静态代码分析与 Clippy 零警告
cargo clippy --workspace --all-targets -- -D warnings

# 4. Action 脚本与 CLI 契约门禁状态机测试
bash action/scripts/test-action.sh
bash action/scripts/test-cli-contract.sh

# 5. 纯 Rust 依赖零 C-linkage 校验（必须输出为空）
cargo tree -i openssl-sys

# 6. 本仓 Dogfooding 门禁重算校验
#    （.sker/bundle.* 已被 gitignore，须先 compile 重建规则包）
cargo build --release -p sker-cli
./target/release/sker compile --repo .
./target/release/sker check --ci --sarif target/sker.sarif --json target/sker.json --repo .
```

可选：仅在改动发布工作流（`dist-workspace.toml`、release CI）时需要额外安装 `cargo-dist` 并运行 `dist plan` 校验。

---

## 四、本仓 Dogfooding 准则

本项目自身也是 Sker 的第一使用者：
- 仓库根目录由 `.sker/` 下的 Playbook 守护；
- 每次修改如果触发规则，必须提供正当合规处理或合规声明；
- 严禁为了绕过检查而私自删除规则、篡改 `.sker/bundle.*` 产物或伪造 `.sker/baseline.json`。
