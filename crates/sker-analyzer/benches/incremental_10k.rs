use sker_analyzer::Analyzer;
use sker_schema::{
    AnalysisRequest, BundleMetadata, ConsultMetadata, DiffAnalyzer, DiffEntry, DiffStatus,
    EntryFixture, EntryScope, FixturePolarity, FixtureTrigger, Language, Lifecycle, LifecycleKind,
    RuleBundle, RuleEntry, RuleSeverity, RuleSignal, RuleTrack, compute_bundle_sha256,
};
use std::time::{Duration, Instant};

fn sample_rule() -> RuleEntry {
    RuleEntry {
        schema_version: "sker.entry/v1".into(),
        rule_id: "bench.go.no-printf".into(),
        version: "1.0.0".into(),
        title: "Avoid fmt.Printf in hot path".into(),
        domain: "performance".into(),
        description: "fmt.Printf allocates and locks stdout".into(),
        rationale: None,
        severity: RuleSeverity::Block,
        track: RuleTrack::Deterministic,
        scope: EntryScope {
            include_paths: vec!["services/**/*.go".into()],
            exclude_paths: vec![],
        },
        signals: vec![RuleSignal {
            signal_id: "printf".into(),
            languages: vec![Language::Go],
            description: "call to fmt.Printf".into(),
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
                path: "examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go"
                    .into(),
                expected: FixtureTrigger::Trigger,
            },
            EntryFixture {
                fixture_id: "neg".into(),
                polarity: FixturePolarity::Negative,
                path: "examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go"
                    .into(),
                expected: FixtureTrigger::NoTrigger,
            },
        ],
        compliant_alternatives: Some(vec!["logger.Info(...)".into()]),
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
        tags: vec!["go".into()],
    }
}

fn generate_10k_go_file() -> (String, Vec<u32>) {
    let mut lines = Vec::with_capacity(10_000);
    lines.push("package bench".to_string());
    lines.push("import \"fmt\"".to_string());
    lines.push("func process() {".to_string());
    for i in 4..=9998 {
        if i == 5000 || i == 9000 {
            lines.push(format!("\tfmt.Printf(\"debug at line {i}: %s\", \"val\")"));
        } else {
            lines.push(format!("\tvar _v_{i} int = {i} * 2"));
        }
    }
    lines.push("}".to_string());
    lines.push("".to_string());

    let line_indices: Vec<u32> = (1..=10_000).collect();
    let content = lines.join("\n");
    (content, line_indices)
}

fn main() {
    println!("=== Incremental 10k Lines Deterministic Check Benchmark ===");
    let env_info = format!(
        "OS={}, Arch={}, Target={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::var("TARGET").unwrap_or_else(|_| "host".into())
    );
    println!("Environment: {env_info}");

    let rule = sample_rule();
    let rules = vec![rule];
    let sources = vec![];
    let bundle_hash = compute_bundle_sha256(&sources, &rules).unwrap();
    let bundle = RuleBundle {
        schema_version: "sker.bundle/v1".into(),
        bundle_id: bundle_hash.clone(),
        bundle_sha256: bundle_hash,
        sources,
        rules,
        metadata: BundleMetadata {
            created_at: "2026-01-01T00:00:00Z".into(),
            compiler_version: "bench".into(),
        },
    };

    let (content, line_indices) = generate_10k_go_file();
    let diff_entry = DiffEntry {
        path: "services/payments/processor.go".into(),
        status: DiffStatus::Modified,
        old_mode: Some(0o100644),
        new_mode: Some(0o100644),
        old_blob: vec![],
        new_blob: content.into_bytes(),
        added_lines: line_indices,
        modified_lines: vec![],
    };

    let request = AnalysisRequest {
        repository_root: ".".into(),
        base_revision: "base".into(),
        head_revision: "head".into(),
        diff: vec![diff_entry],
        baseline: vec![],
        now: "2026-01-01T00:00:00Z".into(),
    };

    // Warm-up run
    let warmup_report = Analyzer.analyze(&bundle, &request).expect("warmup failed");
    assert_eq!(warmup_report.findings.len(), 2);
    assert_eq!(warmup_report.changed_line_count, 10_000);

    let sample_count = 50usize;
    let mut timings = Vec::with_capacity(sample_count);

    for _ in 0..sample_count {
        let start = Instant::now();
        let report = Analyzer
            .analyze(&bundle, &request)
            .expect("analysis failed");
        let elapsed = start.elapsed();
        assert_eq!(report.findings.len(), 2);
        timings.push(elapsed);
    }

    timings.sort();

    let min = timings[0];
    let p50 = timings[(sample_count as f64 * 0.50) as usize];
    let p90 = timings[(sample_count as f64 * 0.90) as usize];
    let p95 = timings[(sample_count as f64 * 0.95) as usize];
    let p99 = timings[(sample_count as f64 * 0.99).min((sample_count - 1) as f64) as usize];
    let max = timings[sample_count - 1];

    println!("Samples: {sample_count}");
    println!("Min: {:.2?}", min);
    println!("p50: {:.2?}", p50);
    println!("p90: {:.2?}", p90);
    println!("p95: {:.2?}", p95);
    println!("p99: {:.2?}", p99);
    println!("Max: {:.2?}", max);

    let target_bound = Duration::from_millis(200);
    assert!(
        p95 <= target_bound,
        "Incremental 10k deterministic check p95 {:?} strictly exceeded 200ms threshold!",
        p95
    );
    println!(
        "RESULT: PASS (p95 = {:.2?} <= 200ms on 10,000 changed lines)",
        p95
    );
}
