# Security Policy

## 漏洞通报渠道 (Reporting a Vulnerability)

如果您在 Sker 中发现了潜在的安全漏洞，请**不要**直接公开发布 Issue。

请通过以下渠道提交漏洞报告：
- **GitHub Private Vulnerability Reporting**: 在本仓库的 [Security Advisories](https://github.com/ai-timer/sker/security/advisories) 页面创建私密漏洞通报；
- **维护者邮箱**: `security@ai-timer.org`（或通过安全公告私信维护者）。

报告应包含重现漏洞的步骤、概念验证代码（PoC）以及潜在影响范围。维护团队将在 48 小时内确认并响应。

---

## 安全承诺与信任边界 (Security Model & Trust Boundaries)

Sker 的分层防御设计明确界定了各层级的信任假定：

### 1. 效率层与权威层的绝对分离
- **L1（Consult 检索）与 L2（Agent Hook / MCP）属于本地效率层**：旨在尽早为开发者与 Agent 提供反馈、节省上下文，但它们运行在不受信任的本地或客户端环境中。
- **L3（CI 重算）是不可绕过的唯一权威验证中心**：
  - CI 绝不盲目信任本地生成的检查结论；
  - CI 独立检出代码、拉取经签名的权威 Playbook、独立提取 Git diff 并重新计算 AST 规则；
  - 本地由 Agent 提交的凭据仅用于核验 diff 绑定与语义声明签名，若发现内容不一致则立即阻断。

### 2. 规约包防投毒机制 (Playbook Poisoning Defense)
- 集中式规则与关键阻断（BLOCK）条目的更改，在组织级需要遵守双人签批机制（Two-person Review / CODEOWNERS 保护）；
- 确定性规则必须提供经过编译验证的 positive/negative 测试夹具（Fixtures），防止引入宽泛误杀或故意放行的恶意规则。
