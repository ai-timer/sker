use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_repo() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "sker-diff-scope-{}-{}-{}",
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

fn git(repo: &PathBuf, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn init_git(repo: &PathBuf) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(
        repo,
        &["config", "user.email", "sker-diff-scope@example.invalid"],
    );
    git(repo, &["config", "user.name", "Sker Diff Scope Test"]);
}

const EVAL_RULE: &str = r#"
schema_version: sker.entry/v1
rule_id: regression.py.no-eval
version: 1.0.0
title: Avoid eval
domain: security
description: eval is forbidden
severity: BLOCK
track: deterministic
scope:
  include_paths: ["**/*.py"]
signals:
  - signal_id: py-eval
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
tags: [python]
"#;

fn write_rule_and_compile(repo: &PathBuf) {
    fs::create_dir_all(repo.join(".sker/rules")).unwrap();
    fs::write(repo.join(".sker/rules/no-eval.yaml"), EVAL_RULE.trim()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["compile", "--repo"])
        .arg(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn check_local(repo: &PathBuf) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["check", "--repo"])
        .arg(repo)
        .output()
        .unwrap()
}

fn assert_blocked(output: &std::process::Output, expected_paths: &[&str]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "local check must exit 1 on deterministic BLOCK; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("\"status\": \"blocked\""),
        "report must be blocked: {stdout}"
    );
    for path in expected_paths {
        assert!(
            stdout.contains(path),
            "finding for {path} missing from report: {stdout}"
        );
    }
}

#[test]
fn unborn_head_with_untracked_violation_is_blocked() {
    let repo = temp_repo();
    init_git(&repo);
    write_rule_and_compile(&repo);

    // No commits yet (unborn HEAD); violation exists only as an untracked file.
    fs::write(repo.join("app.py"), "x = eval(user_input)\n").unwrap();

    let out = check_local(&repo);
    assert_blocked(&out, &["app.py"]);
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn single_root_commit_with_workspace_violations_scans_working_tree() {
    let repo = temp_repo();
    init_git(&repo);
    fs::write(repo.join("app.py"), "x = 1\n").unwrap();
    write_rule_and_compile(&repo);
    git(&repo, &["add", ".sker/rules/no-eval.yaml", "app.py"]);
    git(&repo, &["commit", "-qm", "root commit"]);

    // Unstaged modification of a tracked file plus a brand-new untracked file.
    fs::write(repo.join("app.py"), "x = eval(user_input)\n").unwrap();
    fs::write(repo.join("bad.py"), "y = eval(other)\n").unwrap();

    let out = check_local(&repo);
    assert_blocked(&out, &["app.py", "bad.py"]);
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn multi_commit_repo_scans_workspace_not_only_head_commit() {
    let repo = temp_repo();
    init_git(&repo);
    fs::write(repo.join("clean.py"), "x = 1\n").unwrap();
    write_rule_and_compile(&repo);
    git(&repo, &["add", ".sker/rules/no-eval.yaml", "clean.py"]);
    git(&repo, &["commit", "-qm", "root commit"]);

    // Second commit changes exactly 2 lines (non-violating).
    fs::write(
        repo.join("clean.py"),
        "# comment one\n# comment two\nx = 1\n",
    )
    .unwrap();
    git(&repo, &["add", "clean.py"]);
    git(&repo, &["commit", "-qm", "two line change"]);

    // The only violation is a new untracked workspace file.
    fs::write(repo.join("new.py"), "z = eval(payload)\n").unwrap();

    let out = check_local(&repo);
    assert_blocked(&out, &["new.py"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\"path\": \"clean.py\""),
        "historical non-violating file must not be flagged: {stdout}"
    );

    // Explicit --base with a nonexistent revision must fail clearly.
    let bad_base = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["check", "--repo"])
        .arg(&repo)
        .args(["--base", "no-such-rev-xyz"])
        .output()
        .unwrap();
    assert_eq!(bad_base.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&bad_base.stderr);
    assert!(
        stderr.contains("no-such-rev-xyz"),
        "error must name the invalid revision: {stderr}"
    );
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn ci_on_clean_single_root_commit_falls_back_to_empty_tree() {
    let repo = temp_repo();
    init_git(&repo);
    fs::write(repo.join("clean.py"), "x = 1\n").unwrap();
    write_rule_and_compile(&repo);
    git(&repo, &["add", ".sker/rules/no-eval.yaml", "clean.py"]);
    git(&repo, &["commit", "-qm", "root commit"]);

    // Workspace is dirty with Sker-internal artifacts only; CI must ignore them
    // and compare the root commit against the empty tree without crashing.
    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .args(["check", "--ci", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ci check on root commit must succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(repo.join(".sker/report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "passed");
    assert_eq!(report["analysis"]["complete"], true);
    let _ = fs::remove_dir_all(repo);
}
