# Feature Specification: Sker Preview v2.0

**Feature Branch**: `001-sker-preview`

**Created**: 2026-11-17

**Status**: Draft

**Input**: User description: "$speckit.specify 基于 Sker Release Preview v2.0 目标（服务 20 人多语言团队：Go/Python/TS/YAML），编写功能规约到 `specs/001-sker-preview/spec.md`，包含六个独立验收故事：联邦规则编译；多语言增量检查、基线与回放；Agent MCP/Hooks 与凭据；CI/SARIF 与人机混合审计；初始化、诊断与分发；Ledger 度量。"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - 联邦规则编译与上下文投影 (Priority: P1)

作为规则维护者，我希望从团队与仓库规则来源生成可验证、可复现的规则包和代理指导文件，以便多语言团队共享规则、在规则库增长时仍能保持紧凑上下文。

**Why this priority**: 规则包是本地检查、代理咨询和 CI 重算的共同依据；没有可靠编译结果，后续审计无从建立。

**Independent Test**: 准备包含 `.sker.yaml`、`.sker.lock`、规则覆盖及 fixtures 的独立样例仓，分别测试有效编译、环检测、fixtures 失败和托管文件生成。

**Acceptance Scenarios**:

1. **Given** `.sker.yaml` 声明联邦规则来源且 `.sker.lock` 锁定来源版本，**When** 用户运行 `sker compile`，**Then** 系统校验配置和锁文件并产出 `bundle.rkyv`、`AGENTS.md`、`CLAUDE.md` 及按领域拆分的 `.agents/skills/sker-*/SKILL.md`。
2. **Given** 规则覆盖图为有向无环图且每条规则的全量 fixtures 均通过，**When** 用户运行编译，**Then** 规则包发布成功且可报告来源及内容哈希。
3. **Given** 规则依赖图存在环或任一 fixture 失败，**When** 用户运行编译，**Then** 编译失败并报告环路径或失败 fixture，不覆盖上一有效规则包。
4. **Given** 规则库包含 500 条以上规则，**When** 编译托管上下文文件，**Then** `AGENTS.md` 托管块不超过 300 tokens，`CLAUDE.md` 与之同步，详细规则留在按需技能文件中。

---

### User Story 2 - 增量检查、基线与影子回放 (Priority: P1)

作为 Go、Python、TypeScript 或 YAML/Infra 贡献者，我希望本地检查只评估本次改动，并将静态违规与需要语义判断的规则区分，以便尽早发现问题且不被既有债务或无关提醒打扰。

**Why this priority**: 快速、可预测的本地反馈是规则在日常开发中持续可用的前提。

**Independent Test**: 在包含多语言改动、静态规则、semantic 信号和基线记录的临时仓库运行 `sker check`、`sker baseline` 与 `sker replay --days N`，核对耗时、结果和回放报告。

**Acceptance Scenarios**:

1. **Given** 有不超过 10,000 个新增或修改行的 Git diff，**When** 用户运行 `sker check`，**Then** 系统通过 `gix` 分析 diff 并在 200ms 内完成目标检查，仅将新增/修改行作为本次问题位置。
2. **Given** 静态规则适用于 diff 且存量问题在 `.sker/baseline.json` 中登记，**When** 用户运行检查，**Then** 静态规则按 `ast-grep-core` 规则执行，未改动的基线问题不重复报错，被改动的相关行重新评估。
3. **Given** semantic 规则定义了 `imports` 或 `patterns` 信号，**When** diff 未出现相应信号，**Then** 规则不激活、不产生语义调用；信号出现时，结果标记为需语义判断而非确定性违规。
4. **Given** 用户选择新基线或指定过去 N 天历史，**When** 分别运行 `sker baseline` 或 `sker replay --days N`，**Then** 基线操作记录当前存量问题，回放报告逐规则给出历史触发率与潜在误报，且两者均不修改历史提交。

---

### User Story 3 - Agent 规则路由、Hook 与遵守凭据 (Priority: P1)

作为使用 AI 工具的工程师，我希望 Agent 在行动前检索适用规则、接收高危提示并产生可验证的遵守声明，以便降低违规风险并让后续审查能够验证声明针对的规则和变更。

**Why this priority**: 该能力将项目负面知识送达 Agent，并为 Agent 参与的 PR 提供可审计凭据。

**Independent Test**: 独立启动 `sker mcp` 并调用四项工具，验证检索阈值、四态结果、凭据哈希验证、待审提案与 PreToolUse Hook 提醒。

**Acceptance Scenarios**:

1. **Given** MCP 客户端连接 `sker mcp`，**When** 调用 `consult` 或 `search`，**Then** 系统经 `rmcp` 提供工具；`consult` 使用 `tantivy` 混合检索并应用相关性阈值，低于阈值时返回空命中而不凑数。
2. **Given** 对当前 bundle 和 diff 请求 `attest`，**When** 判断完成，**Then** 返回 `comply`、`not_triggered_in_logic`、`exempt`、`escalate` 中的一个 verdict，并生成 `blake3(bundle_hash || diff_hash || verdicts)` 凭据；bundle、diff 或 verdict 任一改变后凭据验证失败。
3. **Given** Agent 提交规则建议，**When** 调用 `propose_rule`，**Then** 仅生成 pending 候选规则，不使其自动生效。
4. **Given** Agent 通过宿主执行可能涉及高危领域的操作，**When** `sker hook` 处理 `PreToolUse` 事件，**Then** 在毫秒级向宿主注入匹配提醒，并报告 Hook 无法送达的情况。

---

### User Story 4 - CI 独立重算与 PR 人机混合审计 (Priority: P1)

作为 PR 审查者，我希望 CI 从 PR diff 独立重算规则并按贡献方式执行相应审计，以便 Agent 声明得到验证，同时人工贡献者不受 Agent 专用填表要求阻断。

**Why this priority**: CI 是最终合并门禁；独立重算能防止本地状态、遗漏或伪造结果成为合并依据。

**Independent Test**: 分别提交有效 Agent PR、无效或缺凭据 Agent PR、纯人工 PR 和离线环境 PR，核对 SARIF、门禁、自查卡片和遥测失败行为。

**Acceptance Scenarios**:

1. **Given** CI 获得 PR diff 与仓库规则包，**When** 执行 `sker check --ci --sarif`，**Then** 独立重算结果并输出 SARIF v2.1.0 行级标注，且不依赖本地结果或远程中心服务。
2. **Given** GitHub Action 识别出 Agent 参与的 PR，**When** 验证审计材料，**Then** 校验 `blake3` 凭据的完整性及与当前 bundle、diff 和 verdicts 的绑定；缺失或失配时阻断合并。
3. **Given** PR 为纯人工提交，**When** CI 完成检查，**Then** 不要求 Agent 凭据或表单；存在高危语义规则时仅发布 PR 自查卡片，不因此阻断。
4. **Given** CI 已产生审计事件，**When** Ledger 异步上报失败或超时，**Then** 不改变检查结果或阻塞合并。

---

### User Story 5 - 初始化、环境诊断与分发 (Priority: P2)

作为团队成员，我希望工具自动识别项目语言和本机 AI 工具并检查接入状态，以便以最少手工配置开始使用一致的项目规则。

**Why this priority**: 对 20 人、多语言团队而言，简化首次接入与环境排错能减少配置漂移并提高覆盖率。

**Independent Test**: 对含 `go.mod`、`package.json`、`pyproject.toml` 及 YAML/Infra 文件的样例仓运行 init/doctor，并在支持平台的干净环境启动分发物。

**Acceptance Scenarios**:

1. **Given** 仓库中存在语言栈标记文件且本机安装了受支持的 AI 工具，**When** 用户运行 `sker init`，**Then** 自动识别语言和宿主并生成相应配置；重复运行不产生重复托管内容。
2. **Given** 锁文件缺失、内容与规则包不符或环境/宿主配置不可用，**When** 用户运行 `sker doctor`，**Then** 逐项报告环境与锁文件状态并提供修复方向。
3. **Given** 用户下载 `cargo-dist` 为目标平台生成的分发物，**When** 在干净的受支持平台运行核心命令，**Then** 无需外部运行时依赖即可完成初始化、诊断与本地检查。

---

### User Story 6 - 异步治理账本与规则健康度 (Priority: P3)

作为规则治理负责人，我希望 Ledger 异步收集治理事件并呈现规则健康度，以便发现过期或高噪规则并依据数据持续改进，同时遥测服务故障不影响开发和合并。

**Why this priority**: 治理分析帮助团队在规则规模增长后控制噪音，但属于可选的后续价值，核心流程须独立成立。

**Independent Test**: 向单容器 Ledger 提交模拟事件，核验七项指标、SNR、到期及噪音识别；停用 Ledger 后验证 check 和 CI 仍独立完成。

**Acceptance Scenarios**:

1. **Given** Ledger 服务可达且收到脱敏事件，**When** 完成事件汇总，**Then** `apps/sker-ledger` 单容器服务接收事件并提供七项指标：consult 援引率、警告精度、本地覆盖率、合入逃逸率、返工成本比、声明不符率及规则年龄分布。
2. **Given** 规则触发、`not_triggered` 与 `human_override` 数据，**When** Ledger 计算 SNR，**Then** 按 `(not_triggered + human_override) / total_triggers` 返回数值和样本数；总触发数为零时显示无数据。
3. **Given** 规则超过配置到期时间或滚动周均噪音率超过 70%，**When** Ledger 更新健康度，**Then** 分别标记待复审的到期规则与高噪规约，并能查看触发与人工覆写数据；Ledger 不自动停用或发布规则。
4. **Given** Ledger 不可达或上报请求失败，**When** 用户运行本地检查或 CI，**Then** 核心工作流继续完成，失败事件可被标识为未上报。

### Edge Cases

- `.sker.yaml` 或 `.sker.lock` 格式错误、来源无法获取或锁定版本不匹配时，编译须失败并指出具体文件与原因，不得静默采用其他版本。
- fixtures 缺失、无法运行或结果不确定时，不得将其计作通过；失败规则不得进入新 bundle。
- Git diff 为空、浅克隆缺少 `--days N` 所需历史、重命名/删除文件或二进制文件时，须明确报告检查覆盖范围，不得声称完整分析未检查内容。
- baseline 条目过期、损坏或对应行发生变化时，相关代码重新检查；豁免与基线不得形成静默永久放行。
- `imports`/`patterns` 信号配置无效、语言无法识别或文件过大时，须明确标注无法分析，不得解释为信号未触发。
- MCP 检索索引为空或相关性低于阈值时返回空命中；凭据被篡改、复用于不同 diff 或 verdict 顺序/内容不一致时必须验证失败。
- GitHub Action 无法可靠判断 PR 是否有 Agent 参与时，不得静默按纯人工豁免处理；须给出可审计的识别状态并采用明确的失败策略。
- Ledger 收到重复、迟到或缺字段事件时须避免重复计数并标注数据缺口；SNR 零分母显示无数据。
- 热修复规则到期字段无效或缺失时须报告治理错误；遥测停用或服务中断不能影响离线工作流。

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: 系统 MUST 读取 `.sker.yaml` 与 `.sker.lock` 描述的联邦规则及锁定版本，并在编译前校验配置、来源和版本完整性。
- **FR-002**: `sker compile` MUST 检查规则有向依赖图无环，并对规则全量 fixtures 执行回归；任一环或 fixture 失败时拒绝产出新有效 bundle。
- **FR-003**: 成功编译 MUST 产出 `bundle.rkyv`、`AGENTS.md`、`CLAUDE.md` 与领域化 `.agents/skills/sker-*/SKILL.md`；AGENTS 托管块不超过 300 tokens，CLAUDE 托管块须同步。
- **FR-004**: 常驻代理指导的上下文成本 MUST 不随规则总数增长；500 条以上规则仍须通过按需检索获取详细内容。
- **FR-005**: `sker check` MUST 支持 Go、Python、TypeScript 与 YAML/Infra，通过 `gix` 检查 Git diff，并仅将新增/修改行作为本次违规位置。
- **FR-006**: 在不超过 10,000 个新增/修改行的参考 diff 上，`sker check` MUST 在 200ms 内完成增量检查；既有 `.sker/baseline.json` 债务不重复报错，变更行须重新评估。
- **FR-007**: 静态规则 MUST 使用 `ast-grep-core` 规则检查并提供确定性结果；semantic 规则 MUST 根据其 `imports`/`patterns` 信号判断是否激活，未激活时零语义调用，激活时不得伪称静态确定结论。
- **FR-008**: `sker baseline` MUST 创建或更新可审查的存量问题基线；`sker replay --days N` MUST 仅回放指定历史窗口并报告逐规则触发率和潜在误报。
- **FR-009**: `sker mcp` MUST 通过 `rmcp` 暴露 `consult`、`search`、`attest`、`propose_rule`；`consult` MUST 通过 `tantivy` 混合检索并在低于相关性阈值时诚实返回空命中。
- **FR-010**: `sker attest` MUST 使用 `comply`、`not_triggered`、`exempt`、`escalate` 四态之一，并生成与 `blake3(bundle_hash || diff_hash || verdicts)` 等价的凭据；任一输入变化须使旧凭据验证失败。
- **FR-011**: `propose_rule` MUST 仅创建 pending 候选，不得自动生效；规则生效须遵循 Git 中人工审查与合入要求。
- **FR-012**: `sker hook` MUST 适配 `PreToolUse`，并在毫秒级将高危领域提醒交给宿主；失败状态须可见。
- **FR-013**: `sker check --ci --sarif` MUST 在不依赖本地结果或远程中心服务的条件下独立重算 PR diff，并输出 SARIF v2.1.0 行级结果。
- **FR-014**: GitHub Action MUST 对 Agent PR 校验凭据完整性及其对当前 bundle、diff、verdicts 的绑定；无效或必需凭据缺失时阻断。纯人工 PR MUST 免除填表阻断，并在涉及高危语义规则时发布自查卡片。
- **FR-015**: CI 向 Ledger 的上报 MUST 异步执行；失败或超时不得阻断合并或改变权威重算结果。
- **FR-016**: `sker init` MUST 自动识别 `go.mod`、`package.json`、`pyproject.toml` 等语言标记及本机受支持 AI 工具并生成配置；`sker doctor` MUST 检查环境和锁文件并给出逐项状态。
- **FR-017**: 发布物 MUST 使用 `cargo-dist` 提供全平台静态分发；核心工作流在目标平台无需外部运行时依赖。
- **FR-018**: `apps/sker-ledger` MUST 提供单容器服务，接收脱敏事件并统计七项指标：consult 援引率、警告精度、本地覆盖率、合入逃逸率、返工成本比、声明不符率、规则年龄分布。
- **FR-019**: Ledger MUST 按 `(not_triggered + human_override) / total_triggers` 计算 SNR，显示分子、分母与统计窗口；零分母显示无数据，并识别规则到期和滚动周均 SNR >70% 的高噪规约。
- **FR-020**: 核心本地检查、consult 与 CI MUST 不依赖 Ledger 或远程中心服务完成；Ledger 不可用不得阻断核心工作流。
- **FR-021**: 代理不得批准或激活其提出的规则；任何规则生效均须经人工审查并合入 Git。

### Key Entities

- **联邦配置与锁定来源**：规则来源、版本约束、适用范围及完整性信息。
- **规则**：标识、领域、语言/路径作用域、规则类型、触发信号、豁免、优先关系、到期信息和 fixtures。
- **Bundle**：一组校验通过的规则、来源快照与哈希，可供本地、代理和 CI 独立复核。
- **基线条目**：已存在问题的定位和债务状态，随关联改动重新评估。
- **Diff 检查结果**：变更行位置、触发规则、检查类型、结论及建议。
- **Attestation 凭据**：bundle 哈希、diff 哈希、verdicts 与完整性校验值。
- **规则提案**：Agent 生成且等待人工处理的规则候选及其状态。
- **Ledger 事件与指标**：脱敏事件、统计窗口、样本规模、七项治理指标、SNR 及到期/噪音提醒。

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 对最多 10,000 个变更行的参考仓库，至少 95% 的 `sker check` 运行在 200ms 内完成。
- **SC-002**: 在 500 条以上规则的验收仓库中，AGENTS 托管块不超过 300 tokens，常驻指导大小相对于规则数保持固定上限。
- **SC-003**: 所有可编译规则均通过全量 fixtures；注入任意覆盖环时，编译发布成功率为零。
- **SC-004**: 未出现 semantic 信号的检查不产生 semantic 规则触发或语义调用；出现信号时结果清楚标注需要语义判断。
- **SC-005**: CI 对有效 Agent PR 凭据通过验证，对缺失、篡改或绑定错误的凭据 100% 阻断；纯人工 PR 不因缺少 Agent 表单被阻断。
- **SC-006**: 六个用户故事的独立验收场景均可单独完成；Ledger 或网络不可用时本地 check、consult 与 CI 仍可完成。
- **SC-007**: Ledger 能依据同一批输入复算七项指标与 SNR，零分母规则显示无数据，且到期和周均 SNR >70% 的规则均被标记。

## Assumptions

- 团队规模为 20 人，首发语言/配置范围为 Go、Python、TypeScript 与 YAML/Infra。
- 200ms 性能验收以最多 10,000 个变更行为参考上限，并以 p95 测量；硬件、缓存状态及参考仓库在规划阶段固定。
- `imports` 与 `patterns` 是规则维护者显式声明的 semantic 触发信号；信号只决定是否激活语义判断，不代替语义结论。
- “全平台”指 cargo-dist 项目声明支持的发布目标；目标清单在规划和发布验收中固定。
- 七项指标及噪音事件由脱敏事件推导；Ledger 需展示统计窗口和样本数。用户可选择禁用遥测，不影响核心离线流程。
- 规则到期由规则元数据定义；高噪阈值按滚动周窗口计算，严格大于 70% 时触发提醒。
