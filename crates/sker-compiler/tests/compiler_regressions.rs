use sker_compiler::{Compiler, load_playbook, project_guidance, publish_bundle, write_projections};
use sker_schema::{
    BundleCompiler, CompileRequest, RuleBundle, RuleSource, RuleSourceKind, SourceLock,
};
use std::{fs, path::PathBuf};

fn sample() -> RuleSource {
    load_playbook(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/sample-playbook"
    ))
    .expect("sample playbook loads")
}
fn request(sources: Vec<RuleSource>) -> CompileRequest {
    CompileRequest {
        sources,
        compiler_version: "test".into(),
    }
}
fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("sker-compiler-{name}-{}", std::process::id()))
}

#[test]
fn sample_playbook_federated_load_compile_and_archive_roundtrip() {
    let source = sample();
    assert_eq!(source.rules.len(), 4);
    let central = RuleSource {
        lock: SourceLock {
            kind: RuleSourceKind::CentralPlaybook,
            source_id: "central".into(),
            version: "working-tree".into(),
            content_hash: "0".repeat(64),
        },
        rules: vec![source.rules[0].clone()],
    };
    let local = RuleSource {
        lock: SourceLock {
            kind: RuleSourceKind::RepositoryLocal,
            source_id: "repository".into(),
            version: "working-tree".into(),
            content_hash: "1".repeat(64),
        },
        rules: vec![source.rules[1].clone()],
    };
    let bundle = Compiler.compile(&request(vec![central, local])).unwrap();
    assert_eq!(bundle.rules.len(), 2);
    let out = temp("archive");
    publish_bundle(&bundle, &out).unwrap();
    assert!(fs::metadata(&out).unwrap().len() > 0);
    let _ = fs::remove_file(out);
}

#[test]
fn failed_compile_keeps_previous_published_bundle() {
    let bundle = Compiler.compile(&request(vec![sample()])).unwrap();
    let out = temp("preserve");
    publish_bundle(&bundle, &out).unwrap();
    let old = fs::read(&out).unwrap();
    let mut bad = sample();
    let self_id = bad.rules[0].rule_id.clone();
    bad.rules[0].overrides.push(sker_schema::OverrideEdge {
        target_rule_id: self_id,
        relation: sker_schema::OverrideRelation::Supersedes,
        reason: "cycle".into(),
    });
    assert!(Compiler.compile(&request(vec![bad])).is_err());
    assert_eq!(fs::read(&out).unwrap(), old);
    let _ = fs::remove_file(out);
}

#[test]
fn projection_is_identical_bounded_and_generates_domain_skills() {
    let bundle = Compiler.compile(&request(vec![sample()])).unwrap();
    let root = temp("projection");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("AGENTS.md"), "agent intro\n").unwrap();
    fs::write(root.join("CLAUDE.md"), "claude intro\n").unwrap();
    let projection = project_guidance(&bundle, &root).unwrap();
    let managed = |s: &str| {
        s[s.find("<!-- sker:managed:start -->").unwrap()
            ..s.find("<!-- sker:managed:end -->").unwrap() + "<!-- sker:managed:end -->".len()]
            .to_string()
    };
    assert_eq!(
        managed(&projection.agents_md),
        managed(&projection.claude_md)
    );
    assert!(managed(&projection.agents_md).split_whitespace().count() <= 300);
    assert!(
        managed(&projection.agents_md)
            .matches("pinned_to_l1")
            .count()
            <= 3
    );
    assert_eq!(
        managed(&projection.agents_md)
            .lines()
            .filter(|line| line.starts_with("1. ") || line.starts_with("2. "))
            .count(),
        2
    );
    assert!(
        managed(&projection.agents_md).contains("1. ")
            && managed(&projection.agents_md).contains("2. ")
    );
    assert!(
        projection
            .skills
            .keys()
            .any(|key| key.ends_with("/SKILL.md"))
    );
    write_projections(&bundle, &root).unwrap();
    for key in projection.skills.keys() {
        assert!(root.join(key).is_file());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn override_cycle_is_rejected() {
    let mut source = sample();
    let first = source.rules[0].rule_id.clone();
    let second = source.rules[1].rule_id.clone();
    source.rules[0].overrides.push(sker_schema::OverrideEdge {
        target_rule_id: second.clone(),
        relation: sker_schema::OverrideRelation::Supersedes,
        reason: "a".into(),
    });
    source.rules[1].overrides.push(sker_schema::OverrideEdge {
        target_rule_id: first.clone(),
        relation: sker_schema::OverrideRelation::Supersedes,
        reason: "b".into(),
    });
    assert!(matches!(
        Compiler.compile(&request(vec![source])),
        Err(sker_schema::CompileError::OverrideCycle { .. })
    ));
}

#[test]
fn pinned_source_hash_mismatch_fails_closed() {
    let src = sample();
    let bad = SourceLock {
        kind: RuleSourceKind::BusinessLock,
        source_id: "x".into(),
        version: "rev1".into(),
        content_hash: "not-a-digest".into(),
    };
    let locked = RuleSource {
        lock: bad,
        rules: src.rules,
    };
    assert!(Compiler.compile(&request(vec![locked])).is_err());
}

#[allow(dead_code)]
fn _bundle(_: RuleBundle) {}

#[test]
fn repository_override_cannot_weaken_central_block() {
    let source = sample();
    let block = source.rules[0].clone();
    let mut local_rule = source.rules[1].clone();
    local_rule.overrides.push(sker_schema::OverrideEdge {
        target_rule_id: block.rule_id.clone(),
        relation: sker_schema::OverrideRelation::Supersedes,
        reason: "local downgrade attempt".into(),
    });
    let central = RuleSource {
        lock: SourceLock {
            kind: RuleSourceKind::CentralPlaybook,
            source_id: "central".into(),
            version: "working-tree".into(),
            content_hash: "0".repeat(64),
        },
        rules: vec![block],
    };
    let local = RuleSource {
        lock: SourceLock {
            kind: RuleSourceKind::RepositoryLocal,
            source_id: "repository".into(),
            version: "working-tree".into(),
            content_hash: "1".repeat(64),
        },
        rules: vec![local_rule],
    };
    assert!(matches!(
        Compiler.compile(&request(vec![central, local])),
        Err(sker_schema::CompileError::CentralBlockDowngrade { .. })
    ));
}
