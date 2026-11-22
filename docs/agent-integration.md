# AI 编码代理深度集成指南 (Agent Integration)

Sker 专为让各种 AI 编码助手（Codex、Claude Code、Antigravity、Cursor 等）无感遵守团队负面知识而设计。核心原则是：**规则扩充至 500+ 条时，常驻上下文成本严格保持为 $O(1)$**。

---

## 1. 代理两步工作协议 (Two-Step Protocol)

在执行 `sker init` 后，Sker 会按 `.sker.yaml` 中启用的 `agents` 列表，在各接入点的规则文件（默认始终包含 `AGENTS.md`，启用 `claude` 时含 `CLAUDE.md`、启用 `antigravity` 时含 `.agent/rules/sker.md` 等）中幂等注入受管理的微型指令块（< 300 tokens）：

```markdown
<!-- sker:managed:start -->
1. Read the matched rule and its scope.
2. Apply the rule, then run its positive and negative fixtures.
Before changing code: run `sker consult` for the relevant paths.
After changes: run `sker check` and resolve deterministic BLOCK findings.
<!-- sker:managed:end -->
```

这确保了无论 Agent 的上下文窗口有多大，它都不会被冗长的规则说明所淹没，而是通过“按需检索 + 事后校验”的方式高效工作。

---

## 2. 声明式 Agent 集成 (`sker agent`)

Sker 采用**表驱动声明式预设 + 自定义声明**结构适配长尾工具生态。内置预设：`universal` / `claude` / `codex` / `antigravity` / `cursor` / `codebuddy` / `windsurf` / `opencode`。

```bash
# 列出全部内置预设与当前仓库启用状态
sker agent list

# 幂等启用预设（写入 .sker.yaml、生成托管块与 MCP 配置并重新编译投影）
sker agent add codebuddy

# 自定义声明：适配任意冷门/自研工具
sker agent add custom --name inhouse-bot --rules-file .inhouse/rules.md --mcp-file .inhouse/mcp.json

# 移除集成
sker agent remove codebuddy

# 打印标准 5 行通用 mcpServers JSON 片段，供任意 MCP 工具直接复制
sker agent mcp-config
```

`sker init` 的默认行为是克制的：仅生成跨平台通用最小集（`.sker.yaml` + `.sker/rules/` + `AGENTS.md`）并自动完成初始编译；只有仓库内原本已存在对应宿主目录（如 `.claude/`、`.codex/`、`.codebuddy/`、`.cursor/`、`.agent/`）或显式传入 `--agent claude,codebuddy` 时才写入专属配置。仅在本机 `$HOME` 检测到工具时只打印 `hint`，绝不污染共享仓库。

对于不支持 MCP / AGENTS.md 的工具，可用 git 钩子兜底：

```bash
sker init --git-hooks   # 安装 .git/hooks/pre-commit 调用 sker check
```

### 内置预设落盘位置

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

> **默认 `universal` 与显式启用的区别**：未传 `--agent` 且仓内无宿主目录时，fallback 的 `universal` 只投影 `AGENTS.md`，**不会写入 `.mcp.json`**；此时 `sker doctor` 对缺失的 `.mcp.json` 输出 `SKIP optional universal MCP`（绝不判 FAIL）。仅当显式执行 `sker agent add universal`（或 `sker init --agent universal`）时才写入上表中的 MCP 配置；若 `.mcp.json` 已存在但损坏或缺少 `sker` 入口，doctor 仍会判 FAIL。

---

## 3. MCP 协议集成 (Model Context Protocol)

Sker 内置标准的 Model Context Protocol (MCP) 服务器，支持 Claude Desktop、Codex CLI、Cursor 等现代化 Agent 环境。

### 启动与测试 MCP 服务
```bash
sker mcp
```

### 对外暴露的核心 MCP 工具

| 工具名称 | 参数 | 职责描述 |
| :--- | :--- | :--- |
| `sker_consult` | `intent` (string), `paths` (array) | **方案规划前调用**：根据意图与目标文件路径，按需检出相关架构红线与替代方案 |
| `sker_search` | `query` (string) | **离线词法搜索**：通过 Tantivy 离线检索规则库 |
| `sker_attest` | `semantic_verdicts` (object) | **合规凭据签发**：对高危语义规则提供声明，生成绑定的防篡改凭据 |
| `sker_propose` | `entry_yaml` (string), `rationale` (string) | **新规约提案**：由 Agent 提议将刚踩过的新坑沉淀为待审条目 |

### 客户端接入配置示例

#### Claude Code / Claude Desktop (`.claude/settings.json`)
```json
{
  "mcpServers": {
    "sker": {
      "command": "sker",
      "args": ["mcp"]
    }
  }
}
```

#### Codex (`.codex/config.toml`)
```toml
# sker:managed:start
[mcp_servers.sker]
command = "sker"
args = ["mcp"]
# sker:managed:end
```

#### Antigravity (`~/.gemini/antigravity/mcp_config.json`)
```json
{
  "mcpServers": {
    "sker": {
      "command": "sker",
      "args": ["mcp"]
    }
  }
}
```

#### CodeBuddy (`.codebuddy/mcp.json`)
```json
{
  "mcpServers": {
    "sker": {
      "command": "sker",
      "args": ["mcp"]
    }
  }
}
```

#### 任意其他 MCP 工具
运行 `sker agent mcp-config` 获取通用片段直接复制：
```json
{
  "mcpServers": {
    "sker": { "command": "sker", "args": ["mcp"] }
  }
}
```

---

## 4. 编辑器与工具拦截 Hook (`sker hook`)

当 Agent 尝试通过工具直接写入违规代码时，可在 Tool Use 前后挂载 `sker hook` 进行即时拦截：

```bash
# 检查当前工作区，如存在硬性 BLOCK 违规直接退出
sker hook

# 硬阻断模式（返回非零退出码）
sker hook --hard-block
```

在 Claude Code 中可将其配置为 Tool Call 拦截器，一旦生成的文件命中了确定性 `BLOCK` 规则，Hook 会在写入前向 Agent 吐出清晰的错误及建议替换方案，迫使 Agent 自动进行重试修复。

> 在交互终端直接运行 `sker hook` 时，会先向 `stderr` 打印等待提示（正常由 Agent Hook 管道喂入 PreToolUse JSON；手动调试可粘贴 JSON 后按 Ctrl+D / Ctrl+C），`stdout` 的 JSON 输出协议保持不变。

---

## 5. 防篡改 Agent 凭据签发 (`sker attest`)

当代码修改完成并经过本地检查后，Agent 可通过 `sker attest` 生成加密签名的合规凭据：

```bash
# 为本次针对 git diff 的修改生成凭据
sker attest --semantic '[{"rule_id":"cross-service-notification","state":"comply","declaration":"Reviewed logic","evidence_path":"src/service.go","evidence_line":10}]' --output .sker/credential.json
```

### 凭据结构与密码学绑定
生成的 `.sker/credential.json` 包含：
- **`bundle_sha256`**：绑定当前生效的规则包哈希；
- **`diff_tree_hash`**：基于当前 Git 增量变更计算的 BLAKE3 哈希；
- **`semantic_verdicts`**：针对高危语义规则做出的明确遵守声明；
- **`credential_blake3`**：依据 RFC 8785（JSON Canonicalization Scheme）对凭据规范化后计算的 BLAKE3 摘要。

> **防篡改保障**：如果人类或恶意脚本在签发后篡改了凭据中的任何字段，或者进一步修改了代码导致 Git diff 发生变化，CI 权威门禁在重新计算时将发现哈希不一致，立刻予以阻断！
