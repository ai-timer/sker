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
| **`1`** | **Blocked** | 命中确定性 `BLOCK` 规则，或 Agent 凭据缺失/签名失效/被篡改。**CI 中一律阻断合并**。 |
| **`2`** | **Incomplete** | 命中了需人工确认的高危语义规则（`pending_semantic`），或分析不完整。**对人类开发者 Fail-Open（允许合并），对 Agent PR 严格阻断**。 |

---

## 二、核心命令详述

### 1. `sker init`
在目标仓库初始化 Sker 环境。

```bash
sker init [--repo <REPO>]
```
- `--repo <REPO>`: 仓库根目录路径（默认: `.`）
- **动作**：检测仓库语言与 Agent 环境（Claude Code / Codex / Antigravity），生成 `.sker.yaml`，创建 `.sker/rules/`，注入 `AGENTS.md` / `CLAUDE.md` 受控常驻引导块，配置 MCP。

---

### 2. `sker doctor`
检查并诊断当前开发环境的完整性与就绪状态。

```bash
sker doctor [--repo <REPO>]
```
- **动作**：验证 Git 仓库状态、Playbook 规则包有效性、编译器环境及各 Agent 宿主配置。

---

### 3. `sker compile`
将本地规则、Playbook 与 overrides 编译构建为只读规则包与极速离线缓存。

```bash
sker compile [--repo <REPO>] [--update-lock]
```
- `--update-lock`: 当配置了 `extends` 联邦规约源时，锁定或刷新各源 SHA-256 摘要至 `.sker.lock`。若远端源内容漂移且未更新 lock，编译将 fail-closed 拒绝。
- **动作**：验证所有 YAML 格式与独立 `detector_content`，执行有向无环图（DAG）无环检验，拦截中央 BLOCK 局部降级（`CentralBlockDowngrade`），运行确定性规则的正反例夹具（fixtures）回归测试，生成 `.sker/bundle.json` 与 `.sker/bundle.rkyv`，并更新领域技能投影。

---

### 4. `sker check`
对 Git diff 变更行执行增量规约检查与合规判定。

```bash
sker check [OPTIONS]
```
- `--repo <REPO>`: 仓库根目录（默认: `.`）
- `--ci`: 启用 CI 严格权威重算模式（默认输出 SARIF 至 `.sker/report.sarif` 与 CheckReport 至 `.sker/report.json`，亦可由参数覆盖）
- `--sarif <SARIF>`: 指定 SARIF 2.1.0 报告输出文件路径（如 `sker-results.sarif`）
- `--json <JSON>`: 指定 CheckReport 结构化 JSON 报告输出文件路径（如 `sker-check-report.json`）
- `--base <BASE>`: 对比的目标 Git 基线引用（如 `origin/main`，CI 中回退环境变量 `GITHUB_BASE_SHA` 或 `HEAD^`）
- `--credential <CREDENTIAL>`: 指定 Agent 凭据文件路径（默认读取 `.sker/credential.json`）

---

### 5. `sker consult`
方案规划前检索。输入自然语言意图及涉及的文件路径，按需输出匹配的负面知识与规约提示。

```bash
sker consult [OPTIONS] <INTENT> [PATHS]...
```
- `<INTENT>`: 变更意图简述（如 `"refactor authentication using jwt"`）
- `[PATHS]...`: 预计变动的目录或文件路径（如 `src/auth/`）
- `--repo <REPO>`: 仓库根目录（默认: `.`）

---

### 6. `sker search`
基于 Tantivy 的全库离线全文与标签检索。

```bash
sker search [OPTIONS] <QUERY>
```
- `<QUERY>`: 检索关键词或标签（如 `"eval"`, `"sql-injection"`）
- `--repo <REPO>`: 仓库根目录（默认: `.`）

---

### 7. `sker attest`
为当前 Git diff 生成防篡改的 Agent 合规证明凭据（`.sker/credential.json`）。

```bash
sker attest [OPTIONS]
```
- `--semantic <SEMANTIC>`: 针对触发的语义规则提交的合规声明 JSON 数组，格式形如 `'[{"rule_id":"...","state":"comply","declaration":"...","evidence_path":"...","evidence_line":10}]'`（默认: `[]`）
- `--output <OUTPUT>`: 凭据文件输出路径（如 `.sker/credential.json`）
- `--repo <REPO>`: 仓库根目录（默认: `.`）
- **安全保障**：凭据内绑定当前生效规则包与增量 diff 的 BLAKE3 摘要，并使用 RFC 8785 规范化签名。若包含 `escalate` 状态，CI 门禁将阻断退出码 1 拒绝合并。

---

### 8. `sker baseline`
管理存量债务基线快照。

```bash
sker baseline <COMMAND> [--repo <REPO>]
```
- `freeze`: 扫描全仓并将当前存量违规条目冻结至 `.sker/baseline.json`。
- `list`: 列出已在基线中被豁免的存量违规条目。
- `update`: 根据代码移动或重构，更新锚定标记。
- `prune`: 扫描并修剪已在代码中被修复、不再命中的过时基线项。

---

### 9. `sker mcp`
启动遵循 Model Context Protocol (MCP) 规范的标准服务（stdio 传输）。

```bash
sker mcp [--repo <REPO>]
```
- 向 Claude Code、Codex、Antigravity 等客户端暴露 `sker_consult`, `sker_search`, `sker_attest`, `sker_propose` 等工具。

---

### 10. `sker hook`
在 Agent 执行工具调用或保存代码时执行的本地拦截器。

```bash
sker hook [--repo <REPO>] [--hard-block]
```
- `--hard-block`: 命中硬性 `BLOCK` 规则时返回非零状态码阻断写入。

---

### 11. `sker propose`
由 Agent 或工程师快速提交新的团队防踩坑候选规约。

```bash
sker propose [OPTIONS] <ENTRY> <RATIONALE>
```
- `<ENTRY>`: 拟添加的 Entry YAML 文件路径
- `<RATIONALE>`: 增加该规则的背景、事故经验或设计理由

---

### 12. `sker replay`
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
hosts:
  - claude
  - codex
  - antigravity
telemetry:
  endpoint: "https://ledger.internal.example.com/api/v1/events"
  token: "your-token"
rules: []
```

### 2. 本地 Playbook 清单 `.sker/sker.yaml` (`schema_version: sker.playbook/v1`)
```yaml
schema_version: sker.playbook/v1
playbook_id: my-service-guardrails
description: 核心架构红线与合规边界
rules:
  - rules/no-raw-sql.yaml
  - rules/mandatory-telemetry.yaml
```
