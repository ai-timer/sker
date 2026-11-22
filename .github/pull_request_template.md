## 变更概述 (Summary)

<!-- 简要描述本次 PR 解决的问题或新增的特性 -->

## 变更类型 (Type of Change)

- [ ] 🚀 新特性 (New feature)
- [ ] 🐛 缺陷修复 (Bug fix)
- [ ] 📚 文档或规范更新 (Documentation / Specs)
- [ ] 🛡️ 规约库更新 (Playbook / Rule changes)
- [ ] ⚙️ CI / 构建流水线变更 (CI / Build tooling)

## Agent 参与与合规声明 (Sker Compliance & Agent Declaration)

- [ ] **是否由 AI Agent 参与编写**:
  - [ ] 纯人工编写 (`Sker-Actor: human-only`)
  - [ ] Agent 参与编写 (`Sker-Actor: agent`)，并已在 `.sker/credential.json` 附带最新绑定的执行凭据
- [ ] **本地合规门禁**: 本地已运行 `sker check` 且零阻断（BLOCK）报错
- [ ] **高危语义人工核对**: 已人工核对涉及敏感架构边界、加密签名或凭据校验的代码逻辑
- [ ] **规约与 Fixture**: 若涉及规约变动，已补充 positive/negative fixtures 并通过 `sker compile`

## 验证与测试命令 (Verification)

- [ ] `cargo fmt --all -- --check` 格式化通过
- [ ] `cargo test --workspace` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 零警告
- [ ] `bash action/scripts/test-action.sh` 通过（若改动了 action 脚本）
- [ ] `cargo tree -i openssl-sys` 确认零命中
- [ ] `./target/release/sker check --ci --repo .` Dogfooding 通过
