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
    let out = Command::new(env!("CARGO_BIN_EXE_sker"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();
    for command in [
        "compile", "check", "consult", "search", "attest", "propose", "replay", "init", "doctor",
        "baseline", "mcp", "hook",
    ] {
        assert!(help.contains(command), "missing {command}");
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
    git(&["add", "."]);
    git(&["commit", "-qm", "add forbidden log"]);
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
