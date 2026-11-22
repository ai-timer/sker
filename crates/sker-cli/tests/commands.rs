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
        "compile", "check", "consult", "search", "attest", "propose", "replay", "init", "agent",
        "doctor", "baseline", "mcp", "hook",
    ] {
        assert!(help.contains(cmd), "help missing subcommand {cmd}");
    }

    // 3. Verify all subcommands exist and have non-empty about descriptions
    let subcommands = [
        "compile", "check", "consult", "search", "attest", "propose", "replay", "init", "agent",
        "doctor", "baseline", "mcp", "hook",
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

    // 5. Verify agent nested subcommands have non-empty about
    let agent_cmd = app.find_subcommand("agent").expect("missing agent command");
    for sub in ["list", "add", "remove", "mcp-config"] {
        let a_sub = agent_cmd
            .find_subcommand(sub)
            .unwrap_or_else(|| panic!("missing agent subcommand {sub}"));
        assert!(a_sub.get_about().is_some(), "agent {sub} missing about");
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
fn init_hints_for_home_only_antigravity_without_polluting_repo() {
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
    assert!(
        stdout.contains("hint") && stdout.contains("antigravity"),
        "expected a friendly antigravity hint: {stdout}"
    );

    // Home-only detection must NOT write host config or repo-specific files.
    let unchanged_cfg: serde_json::Value =
        serde_json::from_slice(&fs::read(&mcp_cfg).unwrap()).unwrap();
    assert!(
        unchanged_cfg["mcpServers"].as_object().unwrap().is_empty(),
        "home MCP config must stay untouched until explicitly enabled"
    );
    assert!(
        !p.join(".agent/rules/sker.md").exists(),
        "no antigravity rules may be projected without repo enablement"
    );
    let cfg: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&fs::read(p.join(".sker.yaml")).unwrap()).unwrap();
    let agents = cfg["agents"].as_sequence().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].as_str().unwrap(), "universal");

    let _ = fs::remove_dir_all(p);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn agent_add_list_remove_and_custom_declaration_lifecycle() {
    let p = root();
    let init = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["init", "--repo"])
        .arg(&p)
        .output()
        .unwrap();
    assert!(init.status.success());

    // Preset add: codebuddy writes .codebuddy/rules/sker.md + .codebuddy/mcp.json
    let add = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["agent", "--repo"])
        .arg(&p)
        .args(["add", "codebuddy"])
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let rules = fs::read_to_string(p.join(".codebuddy/rules/sker.md")).unwrap();
    assert_eq!(rules.matches("sker:managed:start").count(), 1);
    let mcp: serde_json::Value =
        serde_json::from_slice(&fs::read(p.join(".codebuddy/mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["sker"]["command"], "sker");

    // Idempotent re-add keeps a single managed block and config entry
    let add2 = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["agent", "--repo"])
        .arg(&p)
        .args(["add", "codebuddy"])
        .output()
        .unwrap();
    assert!(add2.status.success());
    let rules = fs::read_to_string(p.join(".codebuddy/rules/sker.md")).unwrap();
    assert_eq!(rules.matches("sker:managed:start").count(), 1);

    // Custom declaration
    let custom = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["agent", "--repo"])
        .arg(&p)
        .args([
            "add",
            "custom",
            "--name",
            "inhouse",
            "--rules-file",
            ".inhouse/rules.md",
            "--mcp-file",
            ".inhouse/mcp.json",
        ])
        .output()
        .unwrap();
    assert!(
        custom.status.success(),
        "{}",
        String::from_utf8_lossy(&custom.stderr)
    );
    assert!(p.join(".inhouse/rules.md").exists());
    assert!(p.join(".inhouse/mcp.json").exists());

    // list shows enabled presets and the custom entry
    let list = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["agent", "--repo"])
        .arg(&p)
        .arg("list")
        .output()
        .unwrap();
    assert!(list.status.success());
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(listing.contains("codebuddy") && listing.contains("[enabled]"));
    assert!(listing.contains("inhouse"));

    // Existing rules and baseline artifacts are untouched by registration
    assert!(p.join(".sker/rules").is_dir());

    // remove drops the entry from .sker.yaml
    let remove = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["agent", "--repo"])
        .arg(&p)
        .args(["remove", "codebuddy"])
        .output()
        .unwrap();
    assert!(remove.status.success());
    let cfg: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&fs::read(p.join(".sker.yaml")).unwrap()).unwrap();
    let names: Vec<&str> = cfg["agents"]
        .as_sequence()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(!names.contains(&"codebuddy"));
    assert!(names.contains(&"universal"));

    // mcp-config prints a copy-pasteable 5-line snippet
    let snippet = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["agent", "mcp-config"])
        .output()
        .unwrap();
    assert!(snippet.status.success());
    let text = String::from_utf8(snippet.stdout).unwrap();
    assert_eq!(text.trim().lines().count(), 5);
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["mcpServers"]["sker"]["args"][0], "mcp");

    let _ = fs::remove_dir_all(p);
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
fn init_doctor_check_all_exit_zero_in_fresh_repository() {
    let p = root();
    let git_init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&p)
        .output()
        .unwrap();
    assert!(
        git_init.status.success(),
        "{}",
        String::from_utf8_lossy(&git_init.stderr)
    );

    let run = |cmd: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_sker"))
            .args([cmd, "--repo"])
            .arg(&p)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "`sker {cmd}` failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    run("init");
    let doctor_stdout = run("doctor");
    assert!(
        doctor_stdout.contains("SKIP optional universal MCP"),
        "doctor must SKIP the absent optional universal MCP: {doctor_stdout}"
    );
    assert!(
        !doctor_stdout.contains("FAIL"),
        "doctor reported FAIL in a freshly initialized repo: {doctor_stdout}"
    );
    run("check");
    let _ = fs::remove_dir_all(p);
}

const BIN_SCOPE_RULE: &str = r#"schema_version: sker.entry/v1
rule_id: regression.bin.no-eval
version: 1.0.0
title: Binary artifacts must not embed eval payloads
domain: security
description: Scoped deterministic rule covering binary artifacts.
severity: BLOCK
track: deterministic
scope:
  include_paths: ["**/*.bin"]
signals:
  - signal_id: bin-eval
    languages: [python]
    description: eval call
    pattern: "eval($$$ARGS)"
exemptions: []
overrides: []
fixtures:
  - fixture_id: eval-call
    polarity: positive
    path: examples/sample-playbook/fixtures/py-no-eval/positive/eval.py
    expected: trigger
  - fixture_id: safe-int-call
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
tags: [binary]
"#;

#[test]
fn init_without_agent_flag_keeps_minimal_set_even_with_populated_home() {
    let p = root();
    let home = root();
    fs::create_dir_all(home.join(".claude")).unwrap();
    fs::create_dir_all(home.join(".codex")).unwrap();
    fs::create_dir_all(home.join(".gemini/antigravity")).unwrap();

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&p)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);

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
    for hint in ["hint: Claude", "hint: Codex", "hint: Antigravity"] {
        assert!(stdout.contains(hint), "missing {hint} in stdout: {stdout}");
    }

    // Exactly the minimal artifact set may exist at the repo root.
    let mut entries: Vec<String> = fs::read_dir(&p)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        [".git", ".gitignore", ".sker", ".sker.yaml", "AGENTS.md"],
        "unexpected root artifacts: {entries:?}"
    );

    // Inside .sker only the rules directory and rebuildable bundle artifacts.
    let mut sker_entries: Vec<String> = fs::read_dir(p.join(".sker"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    sker_entries.sort();
    assert_eq!(
        sker_entries,
        ["bundle.json", "bundle.rkyv", "bundle.sha256", "rules"],
        "unexpected .sker artifacts: {sker_entries:?}"
    );
    assert!(p.join(".sker/rules").is_dir());

    // No host integration may be injected from $HOME detection.
    for forbidden in [
        ".claude",
        ".codex",
        "CLAUDE.md",
        ".mcp.json",
        ".agents",
        ".agent",
        ".cursor",
        ".cursorrules",
        ".codebuddy",
        ".windsurf",
        ".codeium",
        ".opencode",
        "opencode.json",
    ] {
        assert!(
            !p.join(forbidden).exists(),
            "forbidden host artifact generated: {forbidden}"
        );
    }

    let cfg: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&fs::read(p.join(".sker.yaml")).unwrap()).unwrap();
    let agents = cfg["agents"].as_sequence().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].as_str().unwrap(), "universal");

    // Unified ignore list; credential.json must stay committable for CI.
    let ignore = fs::read_to_string(p.join(".gitignore")).unwrap();
    assert!(
        !ignore.contains("credential"),
        "credential.json must never be gitignored"
    );
    for entry in [
        ".sker/bundle.*",
        ".sker/cache/",
        ".sker/tmp/",
        ".sker/check.json",
        ".sker/report.json",
        "*.sarif",
        "sker-report.json",
        "sker-check-report.json",
    ] {
        assert!(
            ignore.lines().any(|l| l.trim() == entry),
            "gitignore missing {entry}"
        );
    }

    let _ = fs::remove_dir_all(p);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn ci_check_writes_both_reports_and_returns_incomplete_exit_code() {
    let repo = root();
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    fs::create_dir_all(repo.join("services/payments")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/sample-playbook/rules/go-no-logging.yaml");
    fs::copy(source, repo.join(".sker/rules/go-no-logging.yaml")).unwrap();
    // Only rule-scoped unsupported files may trigger incomplete; give `.bin`
    // an active deterministic rule scope.
    fs::write(repo.join(".sker/rules/bin-scope.yaml"), BIN_SCOPE_RULE).unwrap();
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
