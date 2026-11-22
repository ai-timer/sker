# 业务仓 5 分钟快速接入指南 (Quickstart)

欢迎使用 **Sker**！本教程将指导你在 5 分钟内为已有或新建的代码仓库接入 Sker 负面知识护栏与 Agent 合规门禁。

---

## 1. 安装 Sker CLI

### 方式一：官方一键安装脚本（推荐）
适用于 macOS 和 Linux（x86_64 及 Apple Silicon / ARM64）：

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ai-timer/sker/releases/download/v0.1.1-preview/sker-installer.sh | sh
```
> 安装脚本会自动检测操作系统与架构，将预编译二进制下载并安装至 `~/.cargo/bin/sker`。请确保 `~/.cargo/bin` 已加入系统 `PATH`。

### 方式二：Homebrew (macOS)
```bash
brew install ai-timer/tap/sker
```

### 方式三：源码编译安装
需要本地具备 Rust 1.85+（Edition 2024）环境：
```bash
git clone https://github.com/ai-timer/sker.git
cd sker
cargo install --path crates/sker-cli
```

验证安装：
```bash
sker --version
```

---

## 2. 仓库初始化 (`sker init`)

进入你的项目根目录，运行初始化命令：

```bash
cd /path/to/your/project
sker init
```

`sker init` 会自动执行以下操作：
1. **识别语言环境**：自动检测项目使用的语言（Go、TypeScript/JavaScript、Python、Rust 等）；
2. **生成跨平台通用最小集**：根目录 `.sker.yaml`（工程主配置）、`.sker/rules/`（本地业务规则目录）与通用 `AGENTS.md` 常驻引导块（不超过 300 tokens）；
3. **幂等维护 `.gitignore`**：追加 Sker 可重建缓存与临时报告（`.sker/bundle.*`、`.sker/cache/`、`.sker/tmp/`、`.sker/check.json`、`.sker/report.json`、`*.sarif`、`sker-report.json`、`sker-check-report.json`）；**绝不忽略 `.sker/credential.json`**（Agent PR 必须提交凭据供 CI 验证）；
4. **自动初始编译**：生成 `.sker/bundle.rkyv`，`init` 后立刻运行 `sker check` / `sker doctor` 零报错可用。

**克制的按需接入**：仅当仓库内原本已存在对应宿主目录（`.claude/`、`.codex/`、`.codebuddy/`、`.cursor/`、`.agent/` 等）时，才写入该工具的专属规则与 MCP 配置；仅在本机 `$HOME` 检测到工具而仓内未启用时，只打印友好 `hint`，绝不污染共享仓库。也可显式启用或后续补充：

```bash
sker init --agent claude,codebuddy   # 显式启用指定集成
sker agent add cursor                # 后续按需补充
sker init --git-hooks                # 安装 pre-commit 钩子兜底任意工具
```

---

## 3. 环境与健康诊断 (`sker doctor`)

运行环境就绪检查，验证 Git、编译器及规则加载状态：

```bash
sker doctor
```

若全部就绪，将显示（默认 `universal` 最小集示例，退出码 0）：
```text
PASS bundle hash
PASS AGENTS.md managed block <= 300 tokens
SKIP optional universal MCP (.mcp.json not present; run 'sker agent add universal' if using an MCP client)
SKIP optional Ledger: SKER_LEDGER_URL not configured
```

> 默认初始化不写入 `.mcp.json`，doctor 对其仅 SKIP 不 FAIL；`sker init` → `sker doctor` → `sker check` 在全新空仓中三者均以退出码 0 通过。

---

## 4. 存量债务冻结与清理 (`sker baseline`)

如果是向成熟的遗留项目中接入 Sker，现有代码中可能已存在存量违规。为了不阻断日常业务迭代，可以将存量问题一次性冻结：

```bash
# 扫描全仓并生成基线快照 .sker/baseline.json
sker baseline freeze

# 查看已冻结的存量条目清单
sker baseline list

# 随业务重构与清理，裁剪已解决的失效条目
sker baseline prune
```

> **注意**：Sker 的 Baseline 采用基于语法 AST 节点与规范化代码片段锚定，**代码行号变动或格式调整不会导致 Baseline 失效**！后续日常检查将只针对 Git diff 新增或修改的行生效。

---

## 5. 本地增量检查 (`sker check`)

在开发者或 Agent 修改代码后，只需毫秒级扫描本次 Git 变更：

```bash
# 检查当前工作区变更（已暂存 + 未暂存 + 未跟踪新文件；工作区干净时回退 parent..HEAD）
sker check

# 输出格式示例：
# 6caf3f1b...: 0 finding(s), 42 changed line(s)
# 退出码 0 代表检查通过；命中确定性 BLOCK 规则时退出码 1（pre-commit 钩子据此拦截）
```

如果在规划变更前需要查询项目是否存在特定限制，可直接运行：
```bash
sker consult "use raw sql query" src/database/
```

---

## 6. GitHub Actions CI 权威门禁接入

在项目根目录下创建 `.github/workflows/sker.yml`：

```yaml
name: Sker Governance Gate

on:
  pull_request:
    branches: [main, master]

permissions:
  contents: read
  pull-requests: write   # 渲染 PR 幂等合规矩阵评论
  issues: write          # PR 评论 API
  security-events: write # 上传 SARIF 报告到 GitHub Security

jobs:
  sker-check:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code
        uses: actions/checkout@v4
        with:
          fetch-depth: 0 # 必须拉取完整历史以计算准确增量 diff

      - name: Run Sker Governance Gate
        uses: ai-timer/sker/action@v0.1.1-preview
        with:
          sker-path: sker
          sarif-file: sker-results.sarif
          report-file: sker-check-report.json
          protected-agent-label: agent-authored
```

至此，你的代码仓库已经具备了企业级 AI 编码代理负面知识防护体系！接下来可参阅：
- [如何编写规约与 Fixtures](writing-rules.md)
- [Agent 集成指南 (Codex/Claude/Cursor)](agent-integration.md)
