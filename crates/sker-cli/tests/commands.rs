use clap::CommandFactory;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
fn root() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "sker-cli-{}-{}-{}",
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
fn clap_exposes_expected_command_tree() {
    // 1. Verify clap command graph invariants and absence of conflicts
    let app = sker_cli::Cli::command();
    app.clone().debug_assert();

    // 2. Verify root help output via CLI executable
    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();

    // Verify slim command help structure
    for cmd in [
        "compile", "check", "consult", "search", "attest", "propose", "replay", "init", "doctor",
        "baseline", "mcp", "hook",
    ] {
        assert!(help.contains(cmd), "help missing subcommand {cmd}");
    }

    // 3. Verify all subcommands exist and have non-empty about descriptions
    let subcommands = [
        "compile", "check", "consult", "search", "attest", "propose", "replay", "init", "doctor",
        "baseline", "mcp", "hook",
    ];
    for command in subcommands {
        assert!(
            help.contains(command),
            "missing command in root help: {command}"
        );
        let sub_out = Command::new(env!("CARGO_BIN_EXE_sker"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(
            sub_out.status.success(),
            "subcommand --help failed for {command}"
        );
        let sub_help = String::from_utf8(sub_out.stdout).unwrap();
        assert!(
            !sub_help.trim().is_empty(),
            "subcommand --help is empty for {command}"
        );
        let sub_app = app
            .find_subcommand(command)
            .unwrap_or_else(|| panic!("missing clap subcommand {command}"));
        assert!(
            sub_app.get_about().is_some() || sub_app.get_long_about().is_some(),
            "subcommand {command} missing about description"
        );
    }

    // 4. Verify baseline nested subcommands have non-empty about
    let baseline_cmd = app
        .find_subcommand("baseline")
        .expect("missing baseline command");
    for sub in ["freeze", "list", "update"] {
        let b_sub = baseline_cmd
            .find_subcommand(sub)
            .unwrap_or_else(|| panic!("missing baseline subcommand {sub}"));
        assert!(b_sub.get_about().is_some(), "baseline {sub} missing about");
    }
}
#[test]
fn init_is_idempotent_in_a_clean_repository() {
    let p = root();
    fs::write(p.join("go.mod"), "module example.invalid/app\n").unwrap();
    for _ in 0..2 {
        let out = Command::new(env!("CARGO_BIN_EXE_sker"))
            .args(["init", "--repo"])
            .arg(&p)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let agents = fs::read_to_string(p.join("AGENTS.md")).unwrap();
    assert_eq!(agents.matches("sker:managed:start").count(), 1);
    assert!(p.join(".sker.yaml").exists());
    let _ = fs::remove_dir_all(p);
}

#[test]
fn init_and_doctor_support_antigravity_host() {
    let p = root();
    fs::create_dir_all(p.join(".agent")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["init", "--repo"])
        .arg(&p)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("antigravity"),
        "missing antigravity in stdout: {stdout}"
    );

    let rule_file = p.join(".agent/rules/sker.md");
    assert!(
        rule_file.exists(),
        ".agent/rules/sker.md should be generated"
    );
    let content = fs::read_to_string(&rule_file).unwrap();
    assert!(content.contains("<!-- sker:managed:start -->"));
    assert!(content.contains("sker consult"));
    assert!(content.contains("sker check"));

    // Also verify .mcp.json was created with sker mcp
    let mcp = p.join(".mcp.json");
    assert!(mcp.exists());
    let mcp_json: serde_json::Value = serde_json::from_slice(&fs::read(&mcp).unwrap()).unwrap();
    assert_eq!(mcp_json["mcpServers"]["sker"]["command"], "sker");

    // Create a minimal bundle so doctor bundle check passes
    fs::create_dir_all(p.join(".sker/rules")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sample-playbook/rules/go-no-logging.yaml");
    fs::copy(source, p.join(".sker/rules/go-no-logging.yaml")).unwrap();
    let compile = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&p)
        .output()
        .unwrap();
    assert!(compile.status.success());

    let doctor = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["doctor", "--repo"])
        .arg(&p)
        .output()
        .unwrap();
    assert!(
        doctor.status.success(),
        "doctor failed: stderr={}, stdout={}",
        String::from_utf8_lossy(&doctor.stderr),
        String::from_utf8_lossy(&doctor.stdout)
    );
    let doc_stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(doc_stdout.contains("PASS Antigravity configuration"));

    let _ = fs::remove_dir_all(p);
}

#[test]
fn init_detects_and_updates_antigravity_home_mcp_config() {
    let p = root();
    let home = root();
    let ag_dir = home.join(".gemini/antigravity");
    fs::create_dir_all(&ag_dir).unwrap();
    let mcp_cfg = ag_dir.join("mcp_config.json");
    fs::write(&mcp_cfg, "{\"mcpServers\":{}}").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["init", "--repo"])
        .arg(&p)
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("antigravity"));

    let updated_cfg: serde_json::Value =
        serde_json::from_slice(&fs::read(&mcp_cfg).unwrap()).unwrap();
    assert_eq!(updated_cfg["mcpServers"]["sker"]["command"], "sker");
    assert_eq!(updated_cfg["mcpServers"]["sker"]["args"][0], "mcp");

    let _ = fs::remove_dir_all(p);
    let _ = fs::remove_dir_all(home);
}
#[test]
fn baseline_list_is_empty_for_clean_repository() {
    let p = root();
    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .arg("baseline")
        .arg("--repo")
        .arg(&p)
        .arg("list")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "[]");
    let _ = fs::remove_dir_all(p);
}

#[test]
fn ci_check_writes_both_reports_and_returns_incomplete_exit_code() {
    let repo = root();
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    fs::create_dir_all(repo.join("services/payments")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sample-playbook/rules/go-no-logging.yaml");
    fs::copy(source, repo.join(".sker/rules/go-no-logging.yaml")).unwrap();
    let file = repo.join("services/payments/main.go");
    fs::write(&file, "package payments\nfunc f() {}\n").unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "sker-test@example.invalid"]);
    git(&["config", "user.name", "Sker Test"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "base"]);
    let base = git(&["rev-parse", "HEAD"]);
    let current =
        "package payments\nimport \"fmt\"\nfunc f() { fmt.Printf(\"secret %s\", \"x\") }\n";
    fs::write(&file, current).unwrap();
    fs::write(repo.join("unsupported.bin"), [0u8, 1u8, 2u8]).unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "add forbidden log and unsupported file"]);
    let compile = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let sarif = repo.join("out.sarif");
    let report = repo.join("out.json");
    let check = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["check", "--ci", "--repo"])
        .arg(&repo)
        .args(["--base", &base, "--sarif"])
        .arg(&sarif)
        .arg("--json")
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(
        check.status.code(),
        Some(2),
        "stderr={} report={}",
        String::from_utf8_lossy(&check.stderr),
        fs::read_to_string(&report).unwrap_or_default()
    );
    assert!(
        report.exists(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(value["schema_version"], "sker.check-report/v1");
    assert_eq!(value["status"], "incomplete");
    assert_eq!(value["analysis"]["complete"], false);
    assert_eq!(value["findings"][0]["disposition"], "blocked");
    assert!(
        sarif.exists(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let sarif: serde_json::Value = serde_json::from_slice(&fs::read(&sarif).unwrap()).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn baseline_anchors_survive_line_shifts_when_header_lines_added() {
    let repo = root();
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    fs::create_dir_all(repo.join("services/payments")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sample-playbook/rules/go-no-logging.yaml");
    fs::copy(source, repo.join(".sker/rules/go-no-logging.yaml")).unwrap();
    let file = repo.join("services/payments/main.go");
    fs::write(
        &file,
        "package payments\nimport \"fmt\"\nfunc f() {\n\tfmt.Printf(\"secret %s\", \"x\")\n}\n",
    )
    .unwrap();

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
    git(&["config", "user.email", "sker-test@example.invalid"]);
    git(&["config", "user.name", "Sker Test"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "initial commit with existing violation"]);

    let compile = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "compile stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let baseline_freeze = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["baseline", "--repo"])
        .arg(&repo)
        .arg("freeze")
        .output()
        .unwrap();
    assert!(
        baseline_freeze.status.success(),
        "baseline freeze stderr={}",
        String::from_utf8_lossy(&baseline_freeze.stderr)
    );

    let baseline_path = repo.join(".sker/baseline.json");
    assert!(baseline_path.exists());
    let baseline_items: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(&baseline_path).unwrap()).unwrap();
    assert_eq!(baseline_items.len(), 1);
    assert_eq!(baseline_items[0]["rule_id"], "go.no-fmt-printf");
    assert_eq!(
        baseline_items[0]["repository_path"],
        "services/payments/main.go"
    );
    assert!(baseline_items[0]["syntax_anchor"].is_string());
    assert!(
        !baseline_items[0]["normalized_fragment"]
            .as_str()
            .unwrap()
            .is_empty()
    );

    git(&["add", "."]);
    git(&["commit", "-qm", "commit compiled bundle and baseline"]);
    let base = git(&["rev-parse", "HEAD"]);

    // Insert 10 lines of header comments at the top of the file, shifting the violation from line ~4 to line ~14,
    // and adjust formatting within the violation call so it is part of the diff hunk while normalized AST fragment remains identical.
    let header = (1..=10)
        .map(|i| format!("// Line {i}: preamble header documentation\n"))
        .collect::<String>();
    let shifted = format!(
        "{header}package payments\nimport \"fmt\"\nfunc f() {{\n\tfmt.Printf(\"secret %s\",  \"x\")\n}}\n"
    );
    fs::write(&file, shifted).unwrap();
    git(&["add", "services/payments/main.go"]);
    git(&["commit", "-qm", "add header comments shifting lines down"]);

    let sarif = repo.join("baseline-check.sarif");
    let report = repo.join("baseline-check.json");
    let check = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["check", "--ci", "--repo"])
        .arg(&repo)
        .args(["--base", &base, "--sarif"])
        .arg(&sarif)
        .arg("--json")
        .arg(&report)
        .output()
        .unwrap();

    assert!(
        report.exists(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(value["schema_version"], "sker.check-report/v1");

    // The finding must still be detected, but disposition must be baseline_suppressed!
    assert_eq!(value["findings"].as_array().unwrap().len(), 1);
    let finding = &value["findings"][0];
    assert_eq!(finding["rule_id"], "go.no-fmt-printf");
    assert_eq!(finding["disposition"], "baseline_suppressed");
    assert!(
        finding["start_line"].as_u64().unwrap() > 10,
        "start line should have shifted down: {}",
        finding["start_line"]
    );
    assert!(finding["baseline_anchor"].is_string());

    // When all deterministic blocking findings are suppressed by baseline and analysis is complete, check succeeds with exit code 0
    assert_eq!(
        check.status.code(),
        Some(0),
        "check should pass with 0 when violation is in baseline; stderr={}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert_eq!(value["status"], "passed");

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn baseline_prune_removes_resolved_violations() {
    let repo = root();
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    fs::create_dir_all(repo.join("services/payments")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sample-playbook/rules/go-no-logging.yaml");
    fs::copy(source, repo.join(".sker/rules/go-no-logging.yaml")).unwrap();
    let file = repo.join("services/payments/main.go");
    fs::write(
        &file,
        "package payments\nimport \"fmt\"\nfunc f() {\n\tfmt.Printf(\"secret %s\", \"x\")\n}\n",
    )
    .unwrap();

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success());
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "sker-test@example.invalid"]);
    git(&["config", "user.name", "Sker Test"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "initial commit"]);

    let compile = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(compile.status.success());

    let freeze = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["baseline", "--repo"])
        .arg(&repo)
        .arg("freeze")
        .output()
        .unwrap();
    assert!(freeze.status.success());

    let baseline_path = repo.join(".sker/baseline.json");
    let baseline_before: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(&baseline_path).unwrap()).unwrap();
    assert_eq!(
        baseline_before.len(),
        1,
        "baseline must contain 1 frozen item"
    );

    // Fix the violation in the file
    fs::write(&file, "package payments\nfunc f() {\n\t// resolved\n}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "fix violation"]);

    // Run baseline prune
    let prune = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["baseline", "--repo"])
        .arg(&repo)
        .arg("prune")
        .output()
        .unwrap();
    assert!(
        prune.status.success(),
        "prune must succeed: {}",
        String::from_utf8_lossy(&prune.stderr)
    );

    let baseline_after: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(&baseline_path).unwrap()).unwrap();
    assert_eq!(
        baseline_after.len(),
        0,
        "resolved item must be pruned from baseline"
    );

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn attest_default_args_does_not_crash_on_parsing() {
    let repo = root();
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sample-playbook/rules/go-no-logging.yaml");
    fs::copy(source, repo.join(".sker/rules/go-no-logging.yaml")).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "sker-test@example.invalid"]);
    git(&["config", "user.name", "Sker Test"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "initial"]);
    fs::write(repo.join("test.txt"), "hello\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "change"]);

    let compile = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(compile.status.success());

    // Calling attest with no --semantic should use default "[]" and succeed without crashing on JSON parsing
    let attest = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["attest", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&attest.stderr);
    assert!(
        !stderr.contains("invalid type: map, expected a sequence"),
        "attest must not fail with map deserialization error: {stderr}"
    );
    assert!(
        attest.status.success(),
        "attest should succeed with 0: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&attest.stdout);
    let cred: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON credential");
    assert_eq!(cred["schema_version"], "sker.agent-credential/v1");

    let _ = fs::remove_dir_all(repo);
}
