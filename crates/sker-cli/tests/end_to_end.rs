use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_repo() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "sker-e2e-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn end_to_end_multilang_changed_lines_baseline_semantic_and_sarif() {
    let repo = temp_repo();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };

    git(&["init", "-q"]);
    git(&["config", "user.email", "sker-e2e@example.invalid"]);
    git(&["config", "user.name", "Sker E2E Test"]);

    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    fs::create_dir_all(repo.join("services/go")).unwrap();
    fs::create_dir_all(repo.join("services/py")).unwrap();
    fs::create_dir_all(repo.join("services/ts")).unwrap();
    fs::create_dir_all(repo.join("infra/k8s")).unwrap();

    // 1. Setup multi-language rules
    // Go deterministic rule
    let go_rule = r#"
schema_version: sker.entry/v1
rule_id: e2e.go.no-print
version: 1.0.0
title: Avoid fmt.Printf in services
domain: observability
description: Use structured logger instead of fmt.Printf
severity: BLOCK
track: deterministic
scope:
  include_paths: ["services/go/**/*.go"]
signals:
  - signal_id: fmt-print
    languages: [go]
    description: print call
    pattern: "fmt.Printf($FMT, $$$ARGS)"
exemptions: []
overrides: []
fixtures:
  - fixture_id: pos
    polarity: positive
    path: examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go
    expected: trigger
  - fixture_id: neg
    polarity: negative
    path: examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go
    expected: no_trigger
compliant_alternatives: ["logger.Info(...)"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: [go]
"#;
    fs::write(repo.join(".sker/rules/go-no-print.yaml"), go_rule.trim()).unwrap();

    // Python deterministic rule
    let py_rule = r#"
schema_version: sker.entry/v1
rule_id: e2e.py.no-eval
version: 1.0.0
title: Avoid eval in python
domain: security
description: eval is forbidden
severity: BLOCK
track: deterministic
scope:
  include_paths: ["services/py/**/*.py"]
signals:
  - signal_id: py-eval
    languages: [python]
    description: eval call
    pattern: "eval($EXPR)"
exemptions: []
overrides: []
fixtures:
  - fixture_id: pos
    polarity: positive
    path: examples/sample-playbook/fixtures/py-no-eval/positive/eval.py
    expected: trigger
  - fixture_id: neg
    polarity: negative
    path: examples/sample-playbook/fixtures/py-no-eval/negative/safe.py
    expected: no_trigger
compliant_alternatives: ["ast.literal_eval(...)"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: [python]
"#;
    fs::write(repo.join(".sker/rules/py-no-eval.yaml"), py_rule.trim()).unwrap();

    // TypeScript deterministic rule
    let ts_rule = r#"
schema_version: sker.entry/v1
rule_id: e2e.ts.no-debugger
version: 1.0.0
title: Avoid debugger statements
domain: quality
description: debugger is forbidden
severity: BLOCK
track: deterministic
scope:
  include_paths: ["services/ts/**/*.ts"]
signals:
  - signal_id: ts-debugger
    languages: [typescript]
    description: debugger keyword
    pattern: "debugger"
exemptions: []
overrides: []
fixtures:
  - fixture_id: pos
    polarity: positive
    path: examples/sample-playbook/fixtures/ts-no-debugger/positive/debug.ts
    expected: trigger
  - fixture_id: neg
    polarity: negative
    path: examples/sample-playbook/fixtures/ts-no-debugger/negative/safe.ts
    expected: no_trigger
compliant_alternatives: ["remove debugger statement"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: [typescript]
"#;
    fs::write(repo.join(".sker/rules/ts-no-debugger.yaml"), ts_rule.trim()).unwrap();

    // YAML-Infra semantic rule (flags pending_semantic)
    let yaml_semantic_rule = r#"
schema_version: sker.entry/v1
rule_id: e2e.infra.privileged-check
version: 1.0.0
title: Review privileged container policies
domain: security
description: Containers with privileged mode require architectural review
severity: BLOCK
track: semantic
scope:
  include_paths: ["infra/**/*.yaml", "infra/**/*.yml"]
signals:
  - signal_id: privileged-container
    languages: [yaml]
    description: privileged true
    pattern: "privileged: true"
exemptions: []
overrides: []
fixtures: []
compliant_alternatives: ["drop capabilities and run as unprivileged"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: [yaml, infra, semantic]
"#;
    fs::write(
        repo.join(".sker/rules/infra-privileged.yaml"),
        yaml_semantic_rule.trim(),
    )
    .unwrap();

    // 2. Initial base commit with an existing Go violation (to be baselined)
    let go_file = repo.join("services/go/main.go");
    fs::write(
        &go_file,
        "package main\nimport \"fmt\"\nfunc main() {\n\tfmt.Printf(\"legacy print %s\", \"v\")\n}\n",
    )
    .unwrap();

    git(&["add", "."]);
    git(&["commit", "-qm", "initial base commit"]);

    // Compile rules into bundle
    let compile = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "compile error: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    // Freeze baseline (should capture services/go/main.go legacy print)
    let baseline_freeze = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["baseline", "--repo"])
        .arg(&repo)
        .arg("freeze")
        .output()
        .unwrap();
    assert!(baseline_freeze.status.success());
    assert!(repo.join(".sker/baseline.json").exists());

    git(&["add", "."]);
    git(&["commit", "-qm", "add compiled bundle and baseline"]);
    let base_rev = git(&["rev-parse", "HEAD"]);

    // 3. Candidate modifications:
    // a) Shift lines in Go file (add top comments) -> baseline should suppress!
    let shifted_go = "// Preamble line 1\n// Preamble line 2\n// Preamble line 3\npackage main\nimport \"fmt\"\nfunc main() {\n\tfmt.Printf(\"legacy print %s\", \"v\")\n}\n";
    fs::write(&go_file, shifted_go).unwrap();

    // b) Python file: add eval (deterministic BLOCK finding)
    let py_file = repo.join("services/py/app.py");
    fs::write(
        &py_file,
        "def run(user_input):\n    return eval(user_input)\n",
    )
    .unwrap();

    // c) TypeScript file: add debugger (deterministic BLOCK finding)
    let ts_file = repo.join("services/ts/index.ts");
    fs::write(
        &ts_file,
        "export function debugMe() {\n    debugger;\n    return 42;\n}\n",
    )
    .unwrap();

    // d) YAML file: add privileged: true (semantic rule finding -> pending_semantic)
    let yaml_file = repo.join("infra/k8s/deploy.yaml");
    fs::write(
        &yaml_file,
        "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: test\n    securityContext:\n      privileged: true\n",
    )
    .unwrap();

    git(&["add", "."]);
    git(&["commit", "-qm", "introduce changes across all languages"]);

    // 4. Run `sker check --ci` and verify reports
    let sarif_path = repo.join("out.sarif");
    let json_path = repo.join("out.json");

    let check = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["check", "--ci", "--repo"])
        .arg(&repo)
        .args(["--base", &base_rev, "--sarif"])
        .arg(&sarif_path)
        .arg("--json")
        .arg(&json_path)
        .output()
        .unwrap();

    // Exit code should be 1 because py and ts have deterministic BLOCK findings!
    assert_eq!(
        check.status.code(),
        Some(1),
        "check should exit 1 for deterministic BLOCK findings; stderr={}",
        String::from_utf8_lossy(&check.stderr)
    );

    // Verify CheckReport JSON
    assert!(json_path.exists());
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(report["schema_version"], "sker.check-report/v1");
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["analysis"]["complete"], true);

    let findings = report["findings"].as_array().expect("findings array");

    // 1) Go finding must be baseline_suppressed
    let go_f = findings.iter().find(|f| f["rule_id"] == "e2e.go.no-print");
    if let Some(f) = go_f {
        assert_eq!(f["disposition"], "baseline_suppressed");
        assert!(f["baseline_anchor"].is_string());
    }

    // 2) Python finding must be blocked
    let py_f = findings
        .iter()
        .find(|f| f["rule_id"] == "e2e.py.no-eval")
        .expect("py eval finding");
    assert_eq!(py_f["disposition"], "blocked");
    assert_eq!(py_f["track"], "deterministic");
    assert_eq!(py_f["severity"], "BLOCK");
    assert_eq!(py_f["path"], "services/py/app.py");

    // 3) TypeScript finding must be blocked
    let ts_f = findings
        .iter()
        .find(|f| f["rule_id"] == "e2e.ts.no-debugger")
        .expect("ts debugger finding");
    assert_eq!(ts_f["disposition"], "blocked");
    assert_eq!(ts_f["track"], "deterministic");
    assert_eq!(ts_f["path"], "services/ts/index.ts");

    // 4) YAML finding must be pending_semantic
    let yaml_f = findings
        .iter()
        .find(|f| f["rule_id"] == "e2e.infra.privileged-check")
        .expect("yaml semantic finding");
    assert_eq!(yaml_f["disposition"], "pending_semantic");
    assert_eq!(yaml_f["track"], "semantic");
    assert_eq!(yaml_f["path"], "infra/k8s/deploy.yaml");

    // Verify SARIF 2.1.0 output
    assert!(sarif_path.exists());
    let sarif: serde_json::Value = serde_json::from_slice(&fs::read(&sarif_path).unwrap()).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    let runs = sarif["runs"].as_array().expect("runs array");
    assert!(!runs.is_empty());
    let results = runs[0]["results"].as_array().expect("results array");
    assert!(!results.is_empty());

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn federated_rules_and_lock_lifecycle_and_central_block_downgrade() {
    let repo = temp_repo();
    let central = temp_repo();

    let git = |args: &[&str], dir: &std::path::Path| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"], &repo);
    git(&["config", "user.email", "test@example.com"], &repo);
    git(&["config", "user.name", "Test"], &repo);

    // Setup central playbook
    fs::create_dir_all(central.join("rules")).unwrap();
    let central_playbook = r#"
schema_version: sker.playbook/v1
playbook_id: central-security
rules:
  - rules/central-block.yaml
"#;
    fs::write(central.join("sker.yaml"), central_playbook.trim()).unwrap();

    let central_rule = r#"
schema_version: sker.entry/v1
rule_id: central.strict-block
version: 1.0.0
title: Central strict block rule
domain: security
description: Central strict block
severity: BLOCK
track: deterministic
scope:
  include_paths: ["services/**/*.go"]
signals:
  - signal_id: sig
    languages: [go]
    description: sig
    pattern: "fmt.Printf($FMT, $$$ARGS)"
exemptions: []
overrides: []
fixtures:
  - fixture_id: pos
    polarity: positive
    path: examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go
    expected: trigger
  - fixture_id: neg
    polarity: negative
    path: examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go
    expected: no_trigger
compliant_alternatives: ["logger.Info(...)"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: []
"#;
    fs::write(
        central.join("rules/central-block.yaml"),
        central_rule.trim(),
    )
    .unwrap();

    // Setup repo extending central
    let central_path_str = central.to_string_lossy().into_owned();
    let repo_config = format!(
        "schema_version: sker.config/v1\nextends:\n  - path: \"{}\"\n    kind: central_playbook\n    source_id: central-security\n",
        central_path_str
    );
    fs::write(repo.join(".sker.yaml"), repo_config).unwrap();
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();

    // 1. Compile auto-generates .sker.lock
    let lock_path = repo.join(".sker.lock");
    assert!(!lock_path.exists());

    let compile_out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        compile_out.status.success(),
        "first compile should succeed and generate lock: {}",
        String::from_utf8_lossy(&compile_out.stderr)
    );
    assert!(lock_path.exists(), ".sker.lock must be auto-generated");
    let lock_content_1 = fs::read_to_string(&lock_path).unwrap();
    assert!(lock_content_1.contains("central-security"));
    assert!(lock_content_1.contains("content_hash"));

    // 2. Normal re-verify passes
    let compile_out_2 = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        compile_out_2.status.success(),
        "subsequent compile should verify lock successfully"
    );

    // 3. Central source modified -> fail-closed rejected
    fs::write(
        central.join("rules/central-block.yaml"),
        format!("{}\n# modified tamper comment\n", central_rule.trim()),
    )
    .unwrap();

    let compile_tampered = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        !compile_tampered.status.success(),
        "compile should fail when central source drifted"
    );
    let tampered_err = String::from_utf8_lossy(&compile_tampered.stderr);
    assert!(
        tampered_err.contains("hash mismatch") && tampered_err.contains("--update-lock"),
        "error must indicate hash mismatch and suggest --update-lock: {tampered_err}"
    );

    // 4. Compile with --update-lock refreshes and succeeds
    let compile_update = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .arg("--update-lock")
        .output()
        .unwrap();
    assert!(
        compile_update.status.success(),
        "compile --update-lock must succeed: {}",
        String::from_utf8_lossy(&compile_update.stderr)
    );
    let lock_content_2 = fs::read_to_string(&lock_path).unwrap();
    assert_ne!(
        lock_content_1, lock_content_2,
        "lock content should have updated"
    );

    // 5. Central BLOCK cannot be downgraded by local repository rule
    let local_downgrade_rule = r#"
schema_version: sker.entry/v1
rule_id: local.downgrade
version: 1.0.0
title: Attempt to downgrade central block to warn
domain: security
description: Local override attempt
severity: WARN
track: deterministic
scope:
  include_paths: ["services/**/*.go"]
signals:
  - signal_id: sig
    languages: [go]
    description: sig
    pattern: "fmt.Printf($FMT, $$$ARGS)"
exemptions: []
overrides:
  - target_rule_id: central.strict-block
    relation: supersedes
    reason: "downgrade to warn"
fixtures:
  - fixture_id: pos
    polarity: positive
    path: examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go
    expected: trigger
  - fixture_id: neg
    polarity: negative
    path: examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go
    expected: no_trigger
compliant_alternatives: ["logger.Info(...)"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: []
"#;
    fs::write(
        repo.join(".sker/rules/downgrade.yaml"),
        local_downgrade_rule.trim(),
    )
    .unwrap();

    let compile_downgrade = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        !compile_downgrade.status.success(),
        "compile must reject local downgrade of central BLOCK"
    );
    let downgrade_err = String::from_utf8_lossy(&compile_downgrade.stderr);
    assert!(
        downgrade_err.contains("cannot be weakened")
            || downgrade_err.contains("cannot be downgraded")
            || downgrade_err.contains("CentralBlockDowngrade"),
        "error must mention downgrade rejection: {downgrade_err}"
    );

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(central);
}

#[test]
fn pre_tool_use_hook_protocol_and_hard_block_filtering() {
    let repo = temp_repo();
    fs::create_dir_all(repo.join(".sker")).unwrap();

    // 1. PreToolUse for sker_* tool must output clean protocol JSON and never block
    let input_sker_consult = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "sker_consult",
        "tool_input": {"intent": "add payment logic"}
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["hook", "--repo"])
        .arg(&repo)
        .arg("--hard-block")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(input_sker_consult.to_string().as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "sker_consult must never be hard-blocked: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let json_val: serde_json::Value = serde_json::from_str(stdout_str.trim()).expect("valid JSON");
    assert!(json_val.get("hookSpecificOutput").is_some());
    assert!(
        json_val.get("delivery_status").is_none(),
        "non-standard wrapper fields must be removed"
    );
    assert!(
        json_val.get("blocked").is_none(),
        "non-standard wrapper fields must be removed"
    );
    assert_eq!(
        json_val["hookSpecificOutput"]["hookEventName"],
        "PreToolUse"
    );

    // 2. Read-only tool targeting sensitive path must not be hard-blocked
    let input_read_auth = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "read_file",
        "tool_input": {"file_path": "src/auth/token.rs"}
    });
    let mut child_read = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["hook", "--repo"])
        .arg(&repo)
        .arg("--hard-block")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child_read.stdin.as_mut().unwrap();
        stdin
            .write_all(input_read_auth.to_string().as_bytes())
            .unwrap();
    }
    let output_read = child_read.wait_with_output().unwrap();
    assert!(
        output_read.status.success(),
        "read-only tool must not be hard-blocked even on auth path"
    );

    // 3. Modifying tool targeting sensitive path without consult: blocked under --hard-block
    let input_edit_auth = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {"file_path": "src/auth/token.rs"}
    });
    let mut child_edit = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["hook", "--repo"])
        .arg(&repo)
        .arg("--hard-block")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child_edit.stdin.as_mut().unwrap();
        stdin
            .write_all(input_edit_auth.to_string().as_bytes())
            .unwrap();
    }
    let output_edit = child_edit.wait_with_output().unwrap();
    assert!(
        !output_edit.status.success(),
        "modifying tool on sensitive path must be hard-blocked when --hard-block is active"
    );

    let _ = fs::remove_dir_all(repo);
}
