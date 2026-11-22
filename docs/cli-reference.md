# Sker CLI 完整命令与配置参考手册 (CLI Reference)

`sker` 是 Sker 离线优先规约引擎与合规门禁的统一命令行入口。

---

## 一、全局语法与退出码规范

```bash
sker [OPTIONS] <COMMAND>
```

### 全局选项
- `-h, --help`: 打印帮助信息
- `-V, --version`: 打印版本号

### 退出码说明

| 退出码 | 语义状态 | 含义与判定说明 |
| :---: | :--- | :--- |
| **`0`** | **Passed** | 检查完全通过，未发现任何阻断性违规；或人机验证成功通过。 |
| **`1`** | **Blocked** | 命中确定性 `BLOCK` 规则，或 Agent 凭据缺失/签名失效/被篡改。本地 `sker check` 与 CI 均阻断（pre-commit 钩子据此拦截提交）。 |
| **`2`** | **Incomplete** | 命中了需人工确认的高危语义规则（`pending_semantic`），或分析不完整。**对人类开发者 Fail-Open（允许合并），对 Agent PR 严格阻断**。 |

---

## 二、核心命令详述

### 1. `sker init`
在目标仓库初始化 Sker 环境（克制式默认，绝不污染共享仓库）。

```bash
sker init [--repo <REPO>] [--agent <LIST>] [--git-hooks]
```
- `--repo <REPO>`: 仓库根目录路径（默认: `.`）
- `--agent <LIST>`: 显式启用的 Agent 集成列表，逗号分隔（如 `--agent claude,codebuddy`）
- `--git-hooks`: 安装轻量 `.git/hooks/pre-commit` 钩子调用 `sker check`，兜底任意不支持 MCP/AGENTS.md 的工具
- **默认动作（未传 `--agent`）**：检测仓库语言，生成 `.sker.yaml` + `.sker/rules/` + 通用 `AGENTS.md` 受控引导块，幂等向 `.gitignore` 追加 Sker 可重建缓存与临时报告（`.sker/bundle.*`、`.sker/cache/`、`.sker/tmp/`、`.sker/check.json`、`.sker/report.json`、`*.sarif`、`sker-report.json`、`sker-check-report.json`；**绝不忽略 `.sker/credential.json`**，Agent PR 必须提交凭据供 CI 验证），并自动执行一次初始编译生成 `.sker/bundle.rkyv`（`init` 后 `sker check` / `doctor` 即刻可用）。
- **按需专属目录**：仅当仓库内原本已存在对应宿主目录（`.claude/`、`.codex/`、`.codebuddy/`、`.cursor/`、`.agent/` 等）或显式传入 `--agent` 时，才写入该工具的专属规则与 MCP 配置；若仅在本机 `$HOME` 检测到工具而仓内未启用，只打印友好 `hint`（可用 `sker agent add <name>` 补充）。

---

### 2. `sker agent`
声明式 Agent 集成管理（表驱动内置预设 + 自定义声明）。

```bash
sker agent list [--repo <REPO>]
sker agent add <NAME> [--repo <REPO>]
sker agent add custom --name <N> --rules-file <PATH> [--mcp-file <PATH>] [--repo <REPO>]
sker agent remove <NAME> [--repo <REPO>]
sker agent mcp-config
```
- `list`: 列出全部内置预设（`universal` / `claude` / `codex` / `antigravity` / `cursor` / `codebuddy` / `windsurf` / `opencode`）及当前仓库启用状态。
- `add <NAME>`: 幂等注册预设进 `.sker.yaml` 的 `agents` 列表，生成对应托管块与 MCP 配置（不破坏现有 `.sker/rules/` 与 `baseline.json`），并重新编译投影。
- `add custom`: 注册自定义声明 `{ name, rules_file, mcp_file }`，适配任意冷门/自研工具。
- `remove <NAME>`: 从 `.sker.yaml` 移除该集成。
- `mcp-config`: 打印标准 5 行通用 `mcpServers` JSON 片段，供任意 MCP 工具直接复制使用。

---

### 3. `sker doctor`
检查并诊断当前开发环境的完整性与就绪状态。

```bash
sker doctor [--repo <REPO>]
```
- **动作**：验证 Git 仓库状态、Playbook 规则包有效性、编译器环境，并按 `.sker.yaml` 已启用的 `agents` 列表逐一核验各接入点的托管块（≤ 300 tokens）与 MCP/Hook 配置健康。

---

### 4. `sker compile`
将本地规则、Playbook 与 overrides 编译构建为只读规则包与极速离线缓存。

```bash
sker compile [--repo <REPO>] [--update-lock]
```
- `--update-lock`: 当配置了 `extends` 联邦规约源时，锁定或刷新各源 SHA-256 摘要至 `.sker.lock`。若远端源内容漂移且未更新 lock，编译将 fail-closed 拒绝。
- **动作**：验证所有 YAML 格式与独立 `detector_content`，执行有向无环图（DAG）无环检验，拦截中央 BLOCK 局部降级（`CentralBlockDowngrade`），运行确定性规则的正反例夹具（fixtures）回归测试，生成 `.sker/bundle.json` 与 `.sker/bundle.rkyv`，并更新领域技能投影。

---

### 5. `sker check`
对 Git diff 变更行执行增量规约检查与合规判定。

```bash
sker check [OPTIONS]
```
- `--repo <REPO>`: 仓库根目录（默认: `.`）
- `--ci`: 启用 CI 严格权威重算模式（默认输出 SARIF 至 `.sker/report.sarif` 与 CheckReport 至 `.sker/report.json`，亦可由参数覆盖）。`.sker/bundle.*` 已被 gitignore，CI 检出后须先运行 `sker compile` 重建规则包（官方 GitHub Action 已自动执行此步骤）。
- `--sarif <SARIF>`: 指定 SARIF 2.1.0 报告输出文件路径（如 `sker-results.sarif`）
- `--json <JSON>`: 指定 CheckReport 结构化 JSON 报告输出文件路径（如 `sker-check-report.json`）
- `--base <BASE>`: 显式对比的目标 Git 基线引用（如 `origin/main`），对比 `<rev>..HEAD`；本地非 `--ci` 时叠加工作区增量；引用不存在时清晰报错。CI 中亦可由环境变量 `GITHUB_BASE_SHA` 提供。
- `--credential <CREDENTIAL>`: 指定 Agent 凭据文件路径（默认读取 `.sker/credential.json`）
- **增量范围判定**：
  - 本地（未传 `--ci` 且未传 `--base`）：只要工作区存在已暂存/未暂存/未跟踪（未被 `.gitignore` 忽略）的改动，即以 `HEAD`（0 次提交时为空树）为基线完整扫描工作区全部新增/修改行；Sker 自身运行产物（`.sker/bundle.*`、`.sker/baseline.json`、`.sker/credential.json` 等）自动跳过。仅当工作区完全干净时才回退对比 `parent..HEAD`（根提交安全回退空树）。
  - CI（`--ci` 且未传 `--base`）：对比 `parent..HEAD`；单根提交或浅克隆找不到父提交时安全回退空树对比。

---

### 6. `sker consult`
方案规划前检索。输入自然语言意图及涉及的文件路径，按需输出匹配的负面知识与规约提示。

```bash
sker consult [OPTIONS] <INTENT> [PATHS]...
```
- `<INTENT>`: 变更意图简述（如 `"refactor authentication using jwt"`）
- `[PATHS]...`: 预计变动的目录或文件路径（如 `src/auth/`）
- `--repo <REPO>`: 仓库根目录（默认: `.`）

---

### 7. `sker search`
基于 Tantivy 的全库离线全文与标签检索。

```bash
sker search [OPTIONS] <QUERY>
```
- `<QUERY>`: 检索关键词或标签（如 `"eval"`, `"sql-injection"`）
- `--repo <REPO>`: 仓库根目录（默认: `.`）

---

### 8. `sker attest`
为当前 Git diff 生成防篡改的 Agent 合规证明凭据（`.sker/credential.json`）。

```bash
sker attest [OPTIONS]
```
- `--semantic <SEMANTIC>`: 针对触发的语义规则提交的合规声明 JSON 数组，格式形如 `'[{"rule_id":"...","state":"comply","declaration":"...","evidence_path":"...","evidence_line":10}]'`（默认: `[]`）
- `--output <OUTPUT>`: 凭据文件输出路径（如 `.sker/credential.json`）
- `--repo <REPO>`: 仓库根目录（默认: `.`）
- **安全保障**：凭据内绑定当前生效规则包与增量 diff 的 BLAKE3 摘要，并使用 RFC 8785 规范化签名。若包含 `escalate` 状态，CI 门禁将阻断退出码 1 拒绝合并。

---

### 9. `sker baseline`
管理存量债务基线快照。

```bash
sker baseline <COMMAND> [--repo <REPO>]
```
- `freeze`: 扫描全仓并将当前存量违规条目冻结至 `.sker/baseline.json`。
- `list`: 列出已在基线中被豁免的存量违规条目。
- `update`: 根据代码移动或重构，更新锚定标记。
- `prune`: 扫描并修剪已在代码中被修复、不再命中的过时基线项。

---

### 10. `sker mcp`
启动遵循 Model Context Protocol (MCP) 规范的标准服务（stdio 传输）。

```bash
sker mcp [--repo <REPO>]
```
- 向 Claude Code、Codex、Antigravity 等客户端暴露 `sker_consult`, `sker_search`, `sker_attest`, `sker_propose` 等工具。

---

### 11. `sker hook`
在 Agent 执行工具调用或保存代码时执行的本地拦截器。

```bash
sker hook [--repo <REPO>] [--hard-block]
```
- `--hard-block`: 命中硬性 `BLOCK` 规则时返回非零状态码阻断写入。
- 在交互终端直接运行时会先向 `stderr` 打印等待提示（正常由 Agent Hook 管道喂入 PreToolUse JSON；可手动粘贴 JSON 后按 Ctrl+D 结束输入），`stdout` JSON 输出协议不变。

---

### 12. `sker propose`
由 Agent 或工程师快速提交新的团队防踩坑候选规约。

```bash
sker propose [OPTIONS] <ENTRY> <RATIONALE>
```
- `<ENTRY>`: 拟添加的 Entry YAML 文件路径
- `<RATIONALE>`: 增加该规则的背景、事故经验或设计理由

---

### 13. `sker replay`
历史影子回放。在两个 Git Commit 历史窗口之间重放当前规则包，评估新增规则对历史提交的影响。

```bash
sker replay [OPTIONS] <FROM> <TO>
```
- `<FROM>`: 起始 Commit 引用
- `<TO>`: 结束 Commit 引用

---

## 三、配置文件结构速查

### 1. 根目录工程配置 `.sker.yaml` (`schema_version: sker.config/v1`)
```yaml
schema_version: sker.config/v1
languages:
  - rust
  - python
  - go
agents:            # 旧键 hosts / integrations 在反序列化时仍兼容
  - universal
  - claude
  - codex
  - antigravity
  - codebuddy
  - name: inhouse-bot        # 自定义声明项
    rules_file: .inhouse/rules.md
    mcp_file: .inhouse/mcp.json
telemetry:
  endpoint: "https://ledger.internal.example.com/api/v1/events"
  token: "your-token"
rules: []
```

内置预设与落盘位置：

| 预设 | 规则文件 | MCP 配置 |
| :--- | :--- | :--- |
| `universal` | `AGENTS.md` | `.mcp.json` |
| `claude` | `CLAUDE.md` | `.claude/settings.json`（含 PreToolUse Hook） |
| `codex` | `AGENTS.md` | `.codex/config.toml` |
| `antigravity` | `.agent/rules/sker.md` | `.mcp.json` |
| `cursor` | `.cursorrules` | `.cursor/mcp.json` |
| `codebuddy` | `.codebuddy/rules/sker.md` | `.codebuddy/mcp.json` |
| `windsurf` | `.windsurfrules` | `.codeium/windsurf/mcp_config.json` |
| `opencode` | `AGENTS.md` | `opencode.json` |

### 2. 本地 Playbook 清单 `.sker/sker.yaml` (`schema_version: sker.playbook/v1`)
```yaml
schema_version: sker.playbook/v1
playbook_id: my-service-guardrails
description: 核心架构红线与合规边界
rules:
  - rules/no-raw-sql.yaml
  - rules/mandatory-telemetry.yaml
```
