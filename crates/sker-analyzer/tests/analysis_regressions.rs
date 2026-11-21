mod fixtures {
    include!("fixtures/sample_sources.rs");
}

use sker_analyzer::{Analyzer, CredentialError, generate_baseline, verify_credential_with_context};
use sker_schema::*;

#[test]
fn language_fixture_matrix_is_present_and_nonempty() {
    for fixture in [
        fixtures::GO_POSITIVE,
        fixtures::GO_NEGATIVE,
        fixtures::PYTHON_POSITIVE,
        fixtures::TYPESCRIPT_POSITIVE,
        fixtures::YAML_INFRA_POSITIVE,
    ] {
        assert!(!fixture.trim().is_empty());
    }
    assert!(fixtures::GO_POSITIVE.contains("fmt.Printf"));
    assert!(fixtures::GO_NEGATIVE.contains("logger.Info"));
}

fn sample_py_rule_with_detector(detector_yaml: &str) -> RuleEntry {
    RuleEntry {
        schema_version: "sker.entry/v1".into(),
        rule_id: "py.no-eval-composite".into(),
        version: "1.0.0".into(),
        title: "Do not use eval or exec in python".into(),
        domain: "security".into(),
        description: "eval/exec executes arbitrary untrusted input".into(),
        rationale: None,
        severity: RuleSeverity::Block,
        track: RuleTrack::Deterministic,
        scope: EntryScope {
            include_paths: vec!["**/*.py".into()],
            exclude_paths: vec![],
        },
        signals: vec![],
        detector: None,
        ast_rule: None,
        detector_content: Some(detector_yaml.into()),
        exemptions: vec![],
        overrides: vec![],
        fixtures: vec![
            EntryFixture {
                fixture_id: "pos".into(),
                polarity: FixturePolarity::Positive,
                path: "fixtures/pos.py".into(),
                expected: FixtureTrigger::Trigger,
            },
            EntryFixture {
                fixture_id: "neg".into(),
                polarity: FixturePolarity::Negative,
                path: "fixtures/neg.py".into(),
                expected: FixtureTrigger::NoTrigger,
            },
        ],
        compliant_alternatives: Some(vec!["Use ast.literal_eval".into()]),
        lifecycle: Lifecycle {
            kind: LifecycleKind::Ordinary,
            cooldown_hours: Some(24),
            hotfix_expires_at: None,
            review_by: None,
        },
        consult: ConsultMetadata {
            synonyms: vec![],
            keywords: vec![],
        },
        tags: vec![],
    }
}

fn make_bundle(rules: Vec<RuleEntry>) -> RuleBundle {
    let digest = compute_bundle_sha256(&[], &rules).unwrap();
    RuleBundle {
        schema_version: "sker.bundle/v1".into(),
        bundle_id: digest.clone(),
        bundle_sha256: digest,
        sources: vec![],
        rules,
        metadata: BundleMetadata {
            created_at: "2026-11-21T00:00:00Z".into(),
            compiler_version: "0.1.0-test".into(),
        },
    }
}

#[test]
fn independent_composite_detector_content_triggers_check_blocking() {
    let yaml = r#"
id: py.no-eval-composite
language: python
rule:
  any:
    - pattern: eval($$$ARGS)
    - pattern: exec($$$ARGS)
"#;
    let rule = sample_py_rule_with_detector(yaml);
    let bundle = make_bundle(vec![rule]);

    let py_code = "def process(user_input):\n    eval(user_input)\n";
    let req = AnalysisRequest {
        repository_root: ".".into(),
        base_revision: "base".into(),
        head_revision: "head".into(),
        diff: vec![DiffEntry {
            path: "services/api/handler.py".into(),
            status: DiffStatus::Added,
            old_mode: None,
            new_mode: Some(0o100644),
            old_blob: vec![],
            new_blob: py_code.as_bytes().to_vec(),
            added_lines: vec![1, 2],
            modified_lines: vec![],
        }],
        baseline: vec![],
        now: "2026-11-21T00:00:00Z".into(),
    };

    let report = Analyzer.analyze(&bundle, &req).unwrap();
    assert_eq!(report.status, CheckStatus::Blocked);
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.rule_id, "py.no-eval-composite");
    assert_eq!(finding.disposition, FindingDisposition::Blocked);
    assert_eq!(finding.start_line, 2);
}

#[test]
fn two_identical_violations_one_baseline_reports_second_line() {
    let rule = RuleEntry {
        schema_version: "sker.entry/v1".into(),
        rule_id: "go.no-printf".into(),
        version: "1.0.0".into(),
        title: "Do not use fmt.Printf".into(),
        domain: "logging".into(),
        description: "Use structured logger".into(),
        rationale: None,
        severity: RuleSeverity::Block,
        track: RuleTrack::Deterministic,
        scope: EntryScope {
            include_paths: vec!["**/*.go".into()],
            exclude_paths: vec![],
        },
        signals: vec![RuleSignal {
            signal_id: "printf".into(),
            languages: vec![Language::Go],
            description: "printf call".into(),
            pattern: "fmt.Printf($FMT, $$$ARGS)".into(),
            detector: None,
            ast_rule: None,
        }],
        detector: None,
        ast_rule: None,
        detector_content: None,
        exemptions: vec![],
        overrides: vec![],
        fixtures: vec![
            EntryFixture {
                fixture_id: "pos".into(),
                polarity: FixturePolarity::Positive,
                path: "pos.go".into(),
                expected: FixtureTrigger::Trigger,
            },
            EntryFixture {
                fixture_id: "neg".into(),
                polarity: FixturePolarity::Negative,
                path: "neg.go".into(),
                expected: FixtureTrigger::NoTrigger,
            },
        ],
        compliant_alternatives: Some(vec!["Use slog".into()]),
        lifecycle: Lifecycle {
            kind: LifecycleKind::Ordinary,
            cooldown_hours: Some(24),
            hotfix_expires_at: None,
            review_by: None,
        },
        consult: ConsultMetadata {
            synonyms: vec![],
            keywords: vec![],
        },
        tags: vec![],
    };
    let bundle = make_bundle(vec![rule]);

    // Freeze baseline with single violation line 4
    let single_line_code =
        "package main\nimport \"fmt\"\nfunc process() {\n    fmt.Printf(\"secret: %s\", x)\n}\n";
    let freeze_req = AnalysisRequest {
        repository_root: ".".into(),
        base_revision: "base".into(),
        head_revision: "head".into(),
        diff: vec![DiffEntry {
            path: "pkg/main.go".into(),
            status: DiffStatus::Added,
            old_mode: None,
            new_mode: Some(0o100644),
            old_blob: vec![],
            new_blob: single_line_code.as_bytes().to_vec(),
            added_lines: vec![1, 2, 3, 4, 5],
            modified_lines: vec![],
        }],
        baseline: vec![],
        now: "2026-11-21T00:00:00Z".into(),
    };
    let baseline_items = generate_baseline(&bundle, &freeze_req).unwrap();
    assert_eq!(baseline_items.len(), 1);

    let two_line_code = "package main\nimport \"fmt\"\nfunc process() {\n    fmt.Printf(\"secret: %s\", x)\n    fmt.Printf(\"secret: %s\", x)\n}\n";
    // Now analyze file with TWO identical violations in the same function
    let check_req = AnalysisRequest {
        repository_root: ".".into(),
        base_revision: "base".into(),
        head_revision: "head".into(),
        diff: vec![DiffEntry {
            path: "pkg/main.go".into(),
            status: DiffStatus::Modified,
            old_mode: Some(0o100644),
            new_mode: Some(0o100644),
            old_blob: single_line_code.as_bytes().to_vec(),
            new_blob: two_line_code.as_bytes().to_vec(),
            added_lines: vec![4, 5],
            modified_lines: vec![],
        }],
        baseline: baseline_items,
        now: "2026-11-21T00:00:00Z".into(),
    };

    let report = Analyzer.analyze(&bundle, &check_req).unwrap();
    assert_eq!(report.findings.len(), 2);
    // Exactly 1 finding is suppressed by the single baseline item quota
    let suppressed = report
        .findings
        .iter()
        .filter(|f| f.disposition == FindingDisposition::BaselineSuppressed)
        .count();
    let blocked = report
        .findings
        .iter()
        .filter(|f| f.disposition == FindingDisposition::Blocked)
        .count();
    assert_eq!(
        suppressed, 1,
        "first occurrence should consume baseline quota 1:1"
    );
    assert_eq!(blocked, 1, "second occurrence must be reported as Blocked");
    assert_eq!(report.status, CheckStatus::Blocked);
}

#[test]
fn expired_incident_hotfix_downgrades_to_warning_with_lifecycle_notice() {
    let mut rule = sample_py_rule_with_detector(
        "id: py.no-eval-composite\nlanguage: python\nrule:\n  pattern: eval($$$ARGS)\n",
    );
    rule.lifecycle = Lifecycle {
        kind: LifecycleKind::IncidentHotfix,
        cooldown_hours: None,
        hotfix_expires_at: Some("2026-01-01T00:00:00Z".into()),
        review_by: None,
    };
    let bundle = make_bundle(vec![rule]);

    let py_code = "eval(x)\n";
    let req = AnalysisRequest {
        repository_root: ".".into(),
        base_revision: "base".into(),
        head_revision: "head".into(),
        diff: vec![DiffEntry {
            path: "main.py".into(),
            status: DiffStatus::Added,
            old_mode: None,
            new_mode: Some(0o100644),
            old_blob: vec![],
            new_blob: py_code.as_bytes().to_vec(),
            added_lines: vec![1],
            modified_lines: vec![],
        }],
        baseline: vec![],
        // now is after hotfix_expires_at
        now: "2026-11-21T00:00:00Z".into(),
    };

    let report = Analyzer.analyze(&bundle, &req).unwrap();
    assert_eq!(report.findings.len(), 1);
    assert_eq!(
        report.findings[0].disposition,
        FindingDisposition::Warning,
        "expired hotfix must not block; must downgrade to warning"
    );
    assert_eq!(report.status, CheckStatus::Passed);
    assert!(
        report
            .analysis
            .warnings
            .iter()
            .any(|w| w.contains("incident hotfix expired")),
        "report must contain lifecycle reminder in warnings"
    );
}

#[test]
fn semantic_verdict_four_states_validation() {
    let diff = vec![DiffEntry {
        path: "src/service.rs".into(),
        status: DiffStatus::Added,
        old_mode: None,
        new_mode: Some(0o100644),
        old_blob: vec![],
        new_blob: b"fn handle() {}\n".to_vec(),
        added_lines: vec![1],
        modified_lines: vec![],
    }];

    let rule = RuleEntry {
        schema_version: "sker.entry/v1".into(),
        rule_id: "sem.audit".into(),
        version: "1.0.0".into(),
        title: "Audit semantic path".into(),
        domain: "audit".into(),
        description: "Semantic check".into(),
        rationale: None,
        severity: RuleSeverity::Block,
        track: RuleTrack::Semantic,
        scope: EntryScope {
            include_paths: vec!["src/**/*.rs".into()],
            exclude_paths: vec![],
        },
        signals: vec![RuleSignal {
            signal_id: "sig".into(),
            languages: vec![Language::Rust],
            description: "sig".into(),
            pattern: "fn handle()".into(),
            detector: None,
            ast_rule: None,
        }],
        detector: None,
        ast_rule: None,
        detector_content: None,
        exemptions: vec![Exemption {
            exemption_id: "ex1".into(),
            paths: vec!["src/service.rs".into()],
            reason: "Legacy service exempted".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            approved_by: "sec-team".into(),
        }],
        overrides: vec![],
        fixtures: vec![],
        compliant_alternatives: None,
        lifecycle: Lifecycle {
            kind: LifecycleKind::Ordinary,
            cooldown_hours: Some(24),
            hotfix_expires_at: None,
            review_by: None,
        },
        consult: ConsultMetadata {
            synonyms: vec![],
            keywords: vec![],
        },
        tags: vec![],
    };
    let bundle = make_bundle(vec![rule]);

    let build_cred = |verdict: SemanticVerdict| {
        let mut c = AgentCredential {
            schema_version: "sker.agent-credential/v1".into(),
            bundle_sha256: bundle.bundle_sha256.clone(),
            diff_tree_hash: compute_diff_tree_hash(&diff),
            consult_trace: vec![],
            semantic_verdicts: vec![verdict],
            issuer: CredentialIssuer::LocalCli,
            created_at: "2026-11-21T00:00:00Z".into(),
            credential_blake3: String::new(),
        };
        c.credential_blake3 = credential_blake3(&c).unwrap();
        c
    };

    // 1. Comply: valid evidence
    let c_comply = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::Comply,
        declaration: "Checked logic and verified compliance".into(),
        rationale_digest: None,
        evidence_path: Some("src/service.rs".into()),
        evidence_line: Some(1),
    });
    assert!(
        verify_credential_with_context(
            &c_comply,
            &c_comply.bundle_sha256,
            &c_comply.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        )
        .is_ok()
    );

    // 1b. Comply: evidence line not in diff -> rejected
    let c_bad_line = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::Comply,
        declaration: "Checked logic".into(),
        rationale_digest: None,
        evidence_path: Some("src/service.rs".into()),
        evidence_line: Some(99),
    });
    assert!(matches!(
        verify_credential_with_context(
            &c_bad_line,
            &c_bad_line.bundle_sha256,
            &c_bad_line.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        ),
        Err(CredentialError::InvalidEvidence(_))
    ));

    // 2. NotTriggeredInLogic: non-empty explanation
    let c_not_trig = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::NotTriggeredInLogic,
        declaration: "Code path is dead / test only".into(),
        rationale_digest: None,
        evidence_path: None,
        evidence_line: None,
    });
    assert!(
        verify_credential_with_context(
            &c_not_trig,
            &c_not_trig.bundle_sha256,
            &c_not_trig.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        )
        .is_ok()
    );

    // 2b. NotTriggeredInLogic: empty explanation -> rejected
    let c_empty_trig = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::NotTriggeredInLogic,
        declaration: "   ".into(),
        rationale_digest: None,
        evidence_path: None,
        evidence_line: None,
    });
    assert!(matches!(
        verify_credential_with_context(
            &c_empty_trig,
            &c_empty_trig.bundle_sha256,
            &c_empty_trig.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        ),
        Err(CredentialError::MissingDeclaration)
    ));

    // 3. Exempt: path matches rule.exemptions
    let c_exempt = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::Exempt,
        declaration: "Covered by ex1 exemption".into(),
        rationale_digest: None,
        evidence_path: Some("src/service.rs".into()),
        evidence_line: None,
    });
    assert!(
        verify_credential_with_context(
            &c_exempt,
            &c_exempt.bundle_sha256,
            &c_exempt.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        )
        .is_ok()
    );

    // 3b. Exempt: path not in exemptions -> rejected
    let c_bad_exempt = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::Exempt,
        declaration: "False exemption claim".into(),
        rationale_digest: None,
        evidence_path: Some("src/unexempted.rs".into()),
        evidence_line: None,
    });
    assert!(matches!(
        verify_credential_with_context(
            &c_bad_exempt,
            &c_bad_exempt.bundle_sha256,
            &c_bad_exempt.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        ),
        Err(CredentialError::InvalidExemption(_))
    ));

    // 4. Escalate: must return RequiresEscalation
    let c_escalate = build_cred(SemanticVerdict {
        rule_id: "sem.audit".into(),
        state: SemanticVerdictState::Escalate,
        declaration: "Ambiguous policy, requesting human security architect review".into(),
        rationale_digest: None,
        evidence_path: None,
        evidence_line: None,
    });
    assert!(matches!(
        verify_credential_with_context(
            &c_escalate,
            &c_escalate.bundle_sha256,
            &c_escalate.diff_tree_hash,
            &["sem.audit".into()],
            Some(&diff),
            Some(&bundle),
        ),
        Err(CredentialError::RequiresEscalation(rules)) if rules == vec!["sem.audit"]
    ));
}
