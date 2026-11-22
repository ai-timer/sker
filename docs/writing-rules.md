# 规约编写与规则生命周期指南 (Writing Rules)

Sker 的核心使命是将工程师团队踩过的坑、架构禁忌与负面知识编译为机器可严格执行的规约文件。

---

## 1. 规则文件概览 (`RuleEntry`)

Sker 规约采用 YAML 格式存储，置于 `.sker/rules/` 目录下（或通过 Playbook 清单引用）。

每条规则对应独立的 `.yaml` 文件，遵循 `sker.entry/v1` 规约规范。

```yaml
schema_version: sker.entry/v1
rule_id: py-no-eval
title: 禁止在生产代码中使用内置 eval 函数
severity: BLOCK
description: 使用 eval 执行字符串极易引入远程代码执行（RCE）高危安全隐患。
track: deterministic
languages:
  - python
scope:
  paths:
    - "**/*.py"
  exclude_paths:
    - "tests/**/*.py"
deterministic:
  engine: ast-grep
  matcher:
    rule:
      pattern: eval($$$ARGS)
alternatives:
  - "使用 ast.literal_eval 解析安全的数据字面量"
  - "针对 JSON 数据使用 json.loads"
fixtures:
  positive: .sker/fixtures/py-no-eval/positive.py
  negative: .sker/fixtures/py-no-eval/negative.py
```

### 核心字段解读

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `schema_version` | String | 必须为 `sker.entry/v1` |
| `rule_id` | String | 全局唯一标识符（建议小写短横线命名，如 `go-no-raw-goroutine`） |
| `title` | String | 简短明了的规则名称 |
| `severity` | Enum | `BLOCK`（硬阻断，合规失败无法合并）、`WARN`（告警提示）、`ADVISE`（优化建议） |
| `track` | Enum | `deterministic`（确定性语法规则）或 `semantic`（高危语义审查规则） |
| `languages` | Array | 目标编程语言列表，如 `["python"]`、`["go"]`、`["typescript"]`、`["rust"]` |
| `scope` | Object | 包含 `paths`（生效路径模式）与 `exclude_paths`（排除路径模式） |
| `alternatives` | Array | 推荐的替代安全写法（向 Agent 投喂的解决方案） |
| `fixtures` | Object | 正例与反例代码夹具路径，**确定性规则必须提供** |
| `lifecycle` | Object | 生命周期配置，支持 `reviewBy`（定期复审截止日期）、`hotfix_expires_at`（突发故障临时红线到期时间）与 `cooldown_hours` |

---

## 2. 确定性语法规则 (`track: deterministic`)

确定性规则由内建纯 Rust `ast-grep` 引擎驱动，仅匹配本次 Git diff 中实际新增或修改的语法节点。

### AST-Grep Matcher 写法
```yaml
track: deterministic
deterministic:
  engine: ast-grep
  matcher:
    rule:
      pattern: fmt.Println($$$ARGS) # 匹配 Go 中的裸打印
```

常用模式占位符：
- `$VAR`：匹配单个 AST 语法节点（变量、表达式、标量）；
- `$$$ARGS`：匹配 0 个或多个语法节点（如参数列表、语句块）。

---

## 3. 高危语义审查规则 (`track: semantic`)

并非所有负面知识都能用单一 AST 结构精准界定（例如“跨服务调用必须注入分布式追踪 TraceID”）。对于涉及上下文理解的约束，Sker 采用启发式语义规则：

```yaml
schema_version: sker.entry/v1
rule_id: cross-service-notification
title: 跨微服务事件推送必须携带 Trace 上下文
severity: WARN
track: semantic
languages:
  - go
  - python
  - typescript
semantic:
  code_signals:
    - "publish_event"
    - "kafka.Produce"
    - "amqp.Publish"
alternatives:
  - "在消息 Header 中透传 context.Context 或 traceparent"
```

**工作机制**：
1. 本地扫描时，仅当 Git diff 增量中命中了 `code_signals` 关键词列表中的模式，才会激活提示；
2. 如果触发，CLI 返回状态码 `2`（需审查）；
3. **人类开发者提交**：采用 Fail-Open 策略，不阻塞合并；
4. **Agent 提交**：必须在 `.sker/credential.json` 中提供对应的 `semantic_verdicts` 明确声明（例如已人工确认或采用合规模式），否则权威 CI 将硬性阻断！

---

## 4. 强制测试夹具与防投毒保障 (Fixtures)

为了杜绝规则书写错误、宽泛误报或恶意放行，Sker 编译器（`sker compile`）强制要求确定性规则附带两类夹具：

1. **`positive`（反例代码 / 违规代码）**：必须**能够被该规则精准捕获**；
2. **`negative`（正例代码 / 推荐代码）**：必须**能够无告警通过**。

```text
.sker/fixtures/py-no-eval/
├── positive.py   # 包含 eval("1 + 1") -> 预期命中
└── negative.py   # 包含 ast.literal_eval("[1, 2]") -> 预期放行
```

编译时若任一夹具极性不符，`sker compile` 将拒绝生成发布包并退出。

---

## 5. 联邦规约源与锁定机制 (Federated Sources & Lockfile)

在大型企业或多团队协作中，基础架构、安全与平台工程团队通常会维护通用的**组织级中央规约包（Central Playbook）**。业务子仓库可以通过项目根目录的 `.sker.yaml` 声明 `extends` 继承中央规则。

### 5.1 `.sker.yaml` 的 `extends` 继承配置

基于 `crates/sker-compiler/src/sources.rs` 的 `ExtendsConfig` 契约，`.sker.yaml` 支持对象形式的详细声明或简写路径形式：

```yaml
schema_version: sker.config/v1
languages:
  - python
  - go
  - rust
extends:
  # 推荐：详细对象格式，显式声明源类型与标识
  - path: ../shared-security-playbook
    kind: central_playbook
    source_id: company-security-playbook
    version: v1.0.0
  # 简写形式（默认解析为 kind: central_playbook）
  # - ../shared-infra-playbook
```

- `path`：引用的外部 Playbook 目录（该目录下需包含 `sker.yaml` 清单文件）或单个规则 `.yaml` 文件路径；
- `kind`：规则源类别，中央规约使用 `central_playbook`；
- `source_id`：规约源唯一标识，缺省时自动使用被引用 Playbook 清单中的 `playbook_id` 或规则文件名；
- `version`：版本标记，默认使用 `working-tree`。

---

### 5.2 首次编译与自动生成 `.sker.lock`

配置 `extends` 后，在项目根目录首次执行标准编译命令：

```bash
sker compile
```

编译器会自动递归解析所继承的 Playbook 清单与全部规则文件，校验所有依赖的 AST Matcher 与 Fixtures 极性，计算整体内容的 SHA-256 摘要，并在当前业务仓根目录自动生成锁定文件 `.sker.lock`：

```yaml
# .sker.lock - 自动生成的联邦规约锁定文件，必须提交至 Git 版本库
schema_version: sker.sources/v1
sources:
  - kind: central_playbook
    source_id: company-security-playbook
    version: v1.0.0
    path: ../shared-security-playbook
    content_hash: 7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069
```

> **安全保障（防内容漂移与投毒）**：`.sker.lock` 必须纳入版本控制。在日常开发与 CI 门禁中，`sker compile` 与 `sker check --ci` 会严格校验引用的外部规则内容哈希是否与 `.sker.lock` 中的 `content_hash` 一致。若外部规则发生静默变更或漂移，编译器将直接 **Fail-Closed 阻断**，防止非预期或恶意的规则投毒直接影响生产业务。

---

### 5.3 升级规约库与刷新锁定 (`sker compile --update-lock`)

当上游中央规约库发布新版本或安全规范更新后，业务仓工程师可显式运行带 `--update-lock` 参数的编译命令进行升级：

```bash
sker compile --update-lock
```

- 编译器将重新扫描全部外部规约源，重新计算新的 SHA-256 哈希并安全覆写 `.sker.lock`；
- 工程师通过 `git diff .sker.lock` 审查内容变动，确认合规后提交合并，完成对组织级规约的受控升级。

---

## 6. 规则覆盖与组织级红线防护 (Overrides)

业务仓库在引入组织级 Central Playbook 时，允许对局部规则进行显式覆盖，但**绝不允许削弱 Central BLOCK 红线**：

```yaml
schema_version: sker.entry/v1
rule_id: local-py-no-eval-strict
overrides:
  - target_rule_id: central-py-no-eval
    rationale: "本仓库提高检查范围，排除 tests 目录以外的全部路径"
```

- **有向无环图验证**：编译器会自动构建 Override 依赖图，如检测到循环覆盖链（`A -> B -> A`），编译将失败。
- **不可降级准则**：中央 Playbook 声明为 `BLOCK` 且无豁免的规则，业务子仓若试图将其降级为 `WARN` 或增加 `exemptions`，编译器直接报错拒绝。

---

## 7. 规则编译与打包 (`sker compile`)

编写或更新规则后，运行编译命令：

```bash
sker compile
```

成功后将产出：
- `.sker/bundle.json`（可读规则包定义及其 BLAKE3/SHA256 哈希）
- `.sker/bundle.rkyv`（极速零拷贝离线缓存）
- 自动更新 `AGENTS.md`、`CLAUDE.md` 与 `.agents/skills/` 领域投影文件。
