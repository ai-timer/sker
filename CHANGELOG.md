# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1-preview] - 2026-11-22

### Changed
- **统一包名为 `sker`**：CLI crate 的 `[package].name` 由 `sker-cli` 改为 `sker`（目录仍为 `crates/sker-cli/`，二进制名仍为 `sker`）。Homebrew 公式统一为 `sker.rb`（`brew install ai-timer/tap/sker`），安装脚本统一为 `sker-installer.sh`，产物为 `sker-<target>.tar.xz`；`cargo-dist` 不再为 Ledger 生成安装器。
- **`sker init` 最小化原则**：未传 `--agent` 时仅生成 `.sker.yaml`、`.sker/rules/`、`AGENTS.md`、`.gitignore` 与 `.sker/bundle.*`；$HOME 探测只打印 `hint`，绝不注入宿主配置；默认 `universal` 不写 `.mcp.json`。
- **`sker doctor`**：默认 `universal` 缺失 `.mcp.json` 时输出 `SKIP optional universal MCP`（不再判 FAIL）；`init` → `doctor` → `check` 在全新空仓全部退出码 0。
- **统一 `.gitignore`**：init 与本仓一致忽略 `.sker/bundle.*`、`.sker/cache/`、`.sker/tmp/`、`.sker/check.json`、`.sker/report.json`、`*.sarif`、`sker-report.json`、`sker-check-report.json`；**绝不忽略 `.sker/credential.json`**（Agent PR 凭据必须可提交）。CI（本仓 workflow 与复合 Action）在 `check --ci` 前先 `sker compile` 重建规则包。
- **Sker Ledger 转为可选组件**：通过 `ghcr.io/ai-timer/sker-ledger` 容器镜像分发（`v*` Tag 触发原生 linux/amd64 构建），`docker-compose.yml` 默认拉取镜像开箱即用；仅使用 `sker` CLI 与 CI 门禁完全无需部署 Ledger 或 PostgreSQL。

### Fixed
- **非规约作用域文件不再误触发 incomplete（exit 2）**：仅当变更文件命中至少一条激活确定性规约的 scope（且未命中 exemptions）却无解析器时才记入 `unsupported_paths`；LICENSE、Dockerfile、.sql、.html 等无覆盖文件不再阻断。

## [0.1.0-preview] - 2026-11-19

Sker 首个公开预览版发布！面向 AI 编码代理（Coding Agents）的负面知识护栏与可审计合规编译器。

### Added
- **Core Architecture & Schema (`sker-schema`)**:
  - 定义 Playbook、Rule Entry、CheckReport、Credential、Baseline 等核心类型；
  - 基于 RFC 8785 规范化 JSON 与 BLAKE3 实现防篡改凭据校验。
- **Compiler & Fixture Harness (`sker-compiler`)**:
  - 支持多源规则解析、无环 DAG 验证及依赖校验；
  - 强制确定性规则正例/反例（positive/negative fixtures）自动化回归。
- **Incremental Analyzer (`sker-analyzer`)**:
  - 基于纯 Rust `gix` 实现毫秒级增量 Git diff 提取；
  - 集成 `ast-grep` 支持 Go、Python、TypeScript、YAML 等多语言语法规则检测。
- **Offline Retriever (`sker-retriever`)**:
  - 基于 Tantivy 构建离线词法与路径标签索引，保持 Agent 常驻上下文 O(1) 成本。
- **Model Context Protocol (`sker-mcp`)**:
  - 实现 MCP 协议规范，为 Coding Agents 提供 `sker_consult` 与 `sker_search` 工具。
- **Unified CLI (`sker-cli`)**:
  - 提供 `compile`, `check`, `consult`, `search`, `attest`, `propose`, `replay`, `init`, `doctor`, `baseline`, `mcp`, `hook` 等 12 个子命令。
- **GitHub Composite Action (`action/`)**:
  - 生产级 PR 门禁，支持 SARIF 上传、幂等合规矩阵评论与 Agent 凭据验证。
- **Ledger Telemetry Server (`apps/sker-ledger`)**:
  - 基于 Axum 与 SQLx 的异步遥测账本，监控规则触发率与信噪比（SNR）。
- **Multi-Platform Release Pipeline**:
  - 基于 `cargo-dist` 自动化跨平台构建（macOS/Linux aarch64 & x86_64）及 Homebrew Tap 发布。
