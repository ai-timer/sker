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

## 二、规范驱动开发（SDD / Spec-Driven Development）

本仓库严格遵循 SDD（Spec-Driven Development）工作流程：

1. **规约先行**：所有新增功能与重大变更须先在 `specs/` 下建立对应功能目录（包含 `spec.md`、`plan.md`、`tasks.md` 等）；
2. **规则防投毒保障**：在 `.sker/` 或添加规约时，必须同时包含对应的正例（positive）与反例（negative）fixtures，并通过 `sker compile` 验证；
3. **架构边界不可侵犯**：各 crate 间数据传递必须严格通过 `sker-schema` 进行，不得跨层直连私有结构。

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

# 4. Action 脚本与门禁状态机测试
bash action/scripts/test-action.sh

# 5. 纯 Rust 依赖零 C-linkage 校验
cargo tree -i openssl-sys

# 6. 发布工作流计划校验
dist plan

# 7. 本仓 Dogfooding 门禁重算校验
cargo build --release -p sker-cli
./target/release/sker check --ci --sarif target/sker.sarif --json target/sker.json --repo .
```

---

## 四、本仓 Dogfooding 准则

本项目自身也是 Sker 的第一使用者：
- 仓库根目录由 `.sker/` 下的 Playbook 守护；
- 每次修改如果触发规则，必须提供正当合规处理或合规声明；
- 严禁为了绕过检查而私自删除规则或篡改 `.sker/bundle.json`。
