use sker_retriever::{IntentRetriever, TantivyRetriever};
use sker_schema::{
    BundleMetadata, ConsultMetadata, ConsultQuery, EntryFixture, EntryScope, FixturePolarity,
    FixtureTrigger, Language, Lifecycle, LifecycleKind, RuleBundle, RuleEntry, RuleSeverity,
    RuleSignal, RuleTrack, SearchQuery, compute_bundle_sha256,
};
use std::time::{Duration, Instant};

fn make_500_rules() -> Vec<RuleEntry> {
    let mut rules = Vec::with_capacity(500);
    let domains = ["security", "observability", "database", "api", "governance"];

    for i in 1..=500 {
        let domain = domains[i % domains.len()];
        let (pattern, lang) = match i % 4 {
            0 => ("fmt.Printf($FMT, $$$ARGS)".to_string(), Language::Go),
            1 => ("eval($EXPR)".to_string(), Language::Python),
            2 => ("debugger".to_string(), Language::Typescript),
            _ => ("privileged: true".to_string(), Language::Yaml),
        };

        let pos_fixture = match lang {
            Language::Go => {
                "examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go"
            }
            Language::Python => "examples/sample-playbook/fixtures/py-no-eval/positive/eval.py",
            Language::Typescript => {
                "examples/sample-playbook/fixtures/ts-no-debugger/positive/debug.ts"
            }
            _ => "examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go",
        };
        let neg_fixture = match lang {
            Language::Go => {
                "examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go"
            }
            Language::Python => "examples/sample-playbook/fixtures/py-no-eval/negative/safe.py",
            Language::Typescript => {
                "examples/sample-playbook/fixtures/ts-no-debugger/negative/safe.ts"
            }
            _ => "examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go",
        };

        rules.push(RuleEntry {
            schema_version: "sker.entry/v1".into(),
            rule_id: format!("rule.{domain}.item_{i:03}"),
            version: "1.0.0".into(),
            title: format!("Rule {i}: safe practices for {domain}"),
            domain: domain.into(),
            description: format!(
                "Ensures compliance with {domain} standards in service subsystem {i}"
            ),
            rationale: Some(format!("Mitigates risk of failures in {domain}")),
            severity: if i % 5 == 0 {
                RuleSeverity::Block
            } else {
                RuleSeverity::Warn
            },
            track: if i % 10 == 0 {
                RuleTrack::Semantic
            } else {
                RuleTrack::Deterministic
            },
            scope: EntryScope {
                include_paths: vec![format!(
                    "services/{domain}/**/*.{ext}",
                    ext = match lang {
                        Language::Go => "go",
                        Language::Python => "py",
                        Language::Typescript => "ts",
                        _ => "yaml",
                    }
                )],
                exclude_paths: vec![],
            },
            signals: vec![RuleSignal {
                signal_id: format!("sig_{i}"),
                languages: vec![lang],
                description: format!("Signal for rule {i}"),
                pattern,
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
                    path: pos_fixture.into(),
                    expected: FixtureTrigger::Trigger,
                },
                EntryFixture {
                    fixture_id: "neg".into(),
                    polarity: FixturePolarity::Negative,
                    path: neg_fixture.into(),
                    expected: FixtureTrigger::NoTrigger,
                },
            ],
            compliant_alternatives: Some(vec![format!("alternative mitigation {i}")]),
            lifecycle: Lifecycle {
                kind: LifecycleKind::Ordinary,
                cooldown_hours: Some(24),
                hotfix_expires_at: None,
                review_by: None,
            },
            consult: ConsultMetadata {
                synonyms: vec![format!("syn_{i}"), format!("{domain}_alias_{i}")],
                keywords: vec![domain.into(), format!("keyword_{i}")],
            },
            tags: if i <= 3 {
                vec!["pinned_to_l1".into(), domain.into()]
            } else {
                vec![domain.into()]
            },
        });
    }
    rules
}

fn main() {
    println!("=== 500 Rules Consult/Search Benchmark & Token Projection ===");
    let env_info = format!(
        "OS={}, Arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("Environment: {env_info}");

    let rules = make_500_rules();
    assert_eq!(rules.len(), 500);

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

    // 1. Projection token context test (must be <= 300 tokens)
    let temp_dir = std::env::temp_dir().join("sker-bench-projection");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let projection =
        sker_compiler::project_guidance(&bundle, &temp_dir).expect("projection failed");
    let tokens = projection.agents_md.split_whitespace().count();
    println!("Projection AGENTS.md token count: {tokens} (assert <= 300)");
    assert!(
        tokens <= 300,
        "projected token count {} exceeds 300 tokens limit",
        tokens
    );
    let _ = std::fs::remove_dir_all(temp_dir);

    // 2. Tantivy Retriever Benchmarks
    let retriever = TantivyRetriever::default();

    let consult_query = ConsultQuery {
        intent: "database connection security and credentials".into(),
        paths: vec!["services/database/client.go".into()],
        limit: Some(10),
        minimum_score: None,
    };

    // Warm-up consult
    let warmup_res = retriever
        .consult(&bundle, &consult_query)
        .expect("warmup consult");
    println!("Warmup consult matched: {} rules", warmup_res.matches.len());
    assert!(
        warmup_res.matches.len() <= 10,
        "consult return size must be bounded <= 10"
    );

    let sample_count = 50usize;
    let mut consult_timings = Vec::with_capacity(sample_count);

    for _ in 0..sample_count {
        let start = Instant::now();
        let res = retriever
            .consult(&bundle, &consult_query)
            .expect("consult failed");
        let elapsed = start.elapsed();
        assert!(res.matches.len() <= 10);
        consult_timings.push(elapsed);
    }

    consult_timings.sort();
    let c_min = consult_timings[0];
    let c_p50 = consult_timings[(sample_count as f64 * 0.50) as usize];
    let c_p95 = consult_timings[(sample_count as f64 * 0.95) as usize];
    let c_max = consult_timings[sample_count - 1];

    println!("\n--- Consult (500 rules) ---");
    println!("Samples: {sample_count}");
    println!("Min: {:.2?}", c_min);
    println!("p50: {:.2?}", c_p50);
    println!("p95: {:.2?}", c_p95);
    println!("Max: {:.2?}", c_max);

    let search_query = SearchQuery {
        query: "observability logging print".into(),
        paths: vec![],
        domains: vec!["observability".into()],
        limit: Some(10),
    };

    let mut search_timings = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let start = Instant::now();
        let res = retriever
            .search(&bundle, &search_query)
            .expect("search failed");
        let elapsed = start.elapsed();
        assert!(res.rules.len() <= 10);
        search_timings.push(elapsed);
    }

    search_timings.sort();
    let s_p95 = search_timings[(sample_count as f64 * 0.95) as usize];
    println!("\n--- Search (500 rules) ---");
    println!("Search p95: {:.2?}", s_p95);

    let limit = Duration::from_millis(200);
    assert!(
        c_p95 <= limit,
        "Consult p95 {:?} strictly exceeded 200ms!",
        c_p95
    );
    assert!(
        s_p95 <= limit,
        "Search p95 {:?} strictly exceeded 200ms!",
        s_p95
    );

    println!(
        "\nRESULT: PASS (500 rules consult p95 = {:.2?}, search p95 = {:.2?}, projection tokens = {})",
        c_p95, s_p95, tokens
    );
}
