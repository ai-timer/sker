# Sker Ledger 遥测与规则健康度治理 (Ledger & Telemetry)

随着团队业务发展，规则库会不断增加。如果不加以度量，规约库可能逐渐充斥着大量过时、误报频发或无人命中的“僵尸规约”，从而演变为团队的负担。

**Sker Ledger** 是一套独立的异步遥测与合规审计账本系统（Axum + SQLx + PostgreSQL），旨在通过**规则信噪比（SNR）**与**生命周期半衰期模型**，持续监控并淘汰衰变规则。

> **可选组件声明**：Sker Ledger 是**可选的团队级度量与看板组件**，通过 `ghcr.io/ai-timer/sker-ledger` 容器镜像分发。**仅使用 `sker` CLI 与 GitHub Action CI 门禁完全无需部署 Ledger 或 PostgreSQL**——所有本地检查、凭据审计与 CI 阻断均独立工作；遥测上报为 fail-open 异步设计，未配置 Ledger 时自动跳过。

---

## 1. 架构与隐私保护原则

1. **绝对隐私脱敏**：上报的事件 Payload **不包含**业务代码正文、文件名细节或作者个人身份，仅包含：
   - 匿名事件 ID 与时间戳（`RFC 3339`）
   - 生效规则包 SHA-256 哈希
   - 本次变更代码行数（`changed_line_count`）
   - 规则 ID 与聚合状态（`pass`, `warn`, `block`）
2. **Fail-Open 异步设计**：遥测上报全程异步且容错，Ledger 服务短暂不可用或网络超时绝不会阻塞本地开发或 CI 流程。

---

## 2. 部署 Sker Ledger

### 方式一：Docker Compose 一键拉起 GHCR 官方镜像（推荐，无需本地编译）

`apps/sker-ledger/docker-compose.yml` 默认直接拉取官方预构建镜像 `ghcr.io/ai-timer/sker-ledger`（推送 `v*` Release Tag 时由 CI 原生 linux/amd64 构建并发布），开箱即用：

```bash
cd apps/sker-ledger

# 1. 配置环境变量
cp .env.example .env
# 编辑 .env 设置高强度密钥
export LEDGER_BEARER_TOKEN="your-secure-telemetry-token"

# 2. 启动 Postgres 与 Ledger 容器（默认镜像标签 v0.1.1-preview，
#    可通过 SKER_LEDGER_VERSION 环境变量覆盖，如 export SKER_LEDGER_VERSION=v0.1.2）
docker compose up -d

# 3. 验证就绪状态
curl -i http://localhost:8080/healthz
# HTTP/1.1 200 OK
# {"status":"ready","database":"connected"}
```

> 若仓库/镜像包为私有，先执行 `docker login ghcr.io -u <github-user> --password-stdin <<< "$GITHUB_TOKEN"`（需 `read:packages` 权限）。

#### 本地源码构建验证（开发者可选）

需要验证本地最新源码时，叠加 `docker-compose.dev.yml` 覆盖文件，从当前工作区源码构建镜像：

```bash
docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d --build
# 或直接运行完整冒烟测试（自动使用 dev 覆盖构建本地源码）：
bash scripts/verify-compose.sh
```

启动后可在浏览器访问治理看板：
👉 **http://localhost:8080/dashboard** （或 `http://localhost:8080/`）

首次进入看板或接口返回 401 时，页面会自动弹出标准的 **Token 解锁面板**：
1. 输入服务端配置的 `LEDGER_BEARER_TOKEN` 并点击 **Unlock Dashboard** 解锁；
2. 凭证安全存储于浏览器的 `sessionStorage` 中，随所有 `/api/v1/*` 请求携带 `Authorization: Bearer <token>`；
3. 支持随时点击右上角 **Lock / Clear Token** 清除会话并重新锁定看板。

---

### 方式二：连接已有 PostgreSQL 服务

```bash
export DATABASE_URL="postgres://user:password@pg-host:5432/sker_ledger"
export LEDGER_BEARER_TOKEN="your-secure-telemetry-token"
export LISTEN_ADDR="0.0.0.0:8080"

# 直接运行二进制
sker-ledger
```

Ledger 启动时会自动应用内置的 SQL 迁移并初始化物化视图。

---

## 3. 端到端遥测接入配置

### 3.1 客户端 CLI 配置 (`.sker.yaml`)
在需要上报本地 `consult` 或 `check` 事件的业务仓库中，在根目录 `.sker.yaml` 中增加配置：

```yaml
schema_version: sker.config/v1
languages:
  - rust
  - python
telemetry:
  endpoint: "https://ledger.internal.your-company.com/api/v1/events"
  token: "your-secure-telemetry-token"
```

### 3.2 GitHub Actions CI 接入
在 `.github/workflows/sker.yml` 中，将 Ledger 配置注入给 Composite Action：

```yaml
      - name: Run Sker Governance Gate
        uses: ai-timer/sker/action@v0.1.1-preview
        with:
          sker-path: sker
          sarif-file: sker-results.sarif
          report-file: sker-check-report.json
          ledger-url: "https://ledger.internal.your-company.com/api/v1/events"
          ledger-token: ${{ secrets.SKER_LEDGER_TOKEN }}
```

---

## 4. 核心治理指标体系 (Overview Metrics)

Ledger 聚合计算以下 7 项团队级核心指标（支持 `7d`, `30d`, `90d` 统计窗口）：

| 指标字段 | 业务含义 | 计算方式 | 治理目标 |
| :--- | :--- | :--- | :--- |
| `consult_citation_rate` | 方案规划前检索引用率 | 检索命中规则数 / 检索调用总次数 | 衡量 Agent 是否在改代码前找到了相关防踩坑指引 |
| `warning_precision` | 真实告警精确度 | 有效命中且已处理数 / (处理数 + 逻辑不触发/豁免数) | 真实告警精度，拒绝拿整体通过率冒充精度 |
| `local_coverage` | 本地护栏捕获占比 | 本地 `check` 检查次数 / (本地 `check` + CI `ci_gate` 拦截总数) | 评估护栏在本地将问题前置捕获的覆盖能力 |
| `merge_escape_rate` | CI 门禁逃逸阻断率 | 在 CI 权威层触发 `BLOCK` 的事件比例 | 越低代表本地护栏拦截越有效（不把问题带到 CI） |
| `rework_cost_ratio` | CI 拦截代码返工成本比 | CI 拦截所涉及的变更代码行数 / 全流程检查变更总行数 | 度量负面知识为团队挽回的代码重构损耗 |
| `declaration_mismatch_rate` | 凭据失配与升级裁决率 | 出现凭据失配或需人工介入（`escalate`）的事件占比 | 发现团队中未经申报的偏离行为与需人工决策规约 |
| `rule_age_distribution` | 规约库年龄与生命周期分布 | 按 `0-30天`、`31-90天`、`91-180天`、`180天以上` 统计规约分布 | 监控规约库健康老化，驱动过期规约与僵尸规约清理 |

---

## 5. 规则信噪比 (SNR) 与生命周期半衰期治理

访问 `GET /api/v1/rules/health` 可获取每一条具体规则的健康状态评估：

```json
{
  "rule_id": "cross-service-notification",
  "status": "noisy",
  "sample_count": 45,
  "noise_rate": 0.78,
  "action": "review",
  "message": "Weekly signal-to-noise rate exceeds 70%; review trigger quality."
}
```

### 5.1 噪音率 (Noise Rate) 计算
$$\text{Noise Rate} = \frac{\text{not\_triggered\_in\_logic} + \text{exempt}}{\text{total\_samples}}$$

- 当一条规则在 7 天统计窗口内的**噪音率超过 70%**（`> 0.70`）时，系统自动打标为 **`noisy`**；
- **治理动作**：提示维护人员优化 AST Matcher 或缩小文件生效路径（`scope.paths`），防止过多假阳性造成警报疲劳。

### 5.2 规则衰变与半衰期淘汰
- **`suspected_dead`（疑似僵尸规约）**：
  连续超过 **180 天**未曾发生任何触发命中的规则。该类规约通常对应早已下线的历史遗留系统或过时的编程习惯，系统建议直接归档下线。
- **`lifecycle_expiring` / `lifecycle_expired`（规约到期与待复审提醒）**：
  用于支持突发故障修复（Hotfix）或架构重构期间的临时红线（通过 `hotfix_expires_at` 或 `cooldown_hours` 约束），以及常规规约的定期复审周期（通过 `reviewBy` 约束）。超期未复审或 hotfix 超期后看板自动打标并提示 `renew` 或 `review`。
