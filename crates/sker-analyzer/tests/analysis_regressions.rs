mod fixtures {
    include!("fixtures/sample_sources.rs");
}

#[test]
fn language_fixture_matrix_is_present_and_nonempty() {
    for fixture in [fixtures::GO_POSITIVE, fixtures::GO_NEGATIVE, fixtures::PYTHON_POSITIVE, fixtures::TYPESCRIPT_POSITIVE, fixtures::YAML_INFRA_POSITIVE] {
        assert!(!fixture.trim().is_empty());
    }
    assert!(fixtures::GO_POSITIVE.contains("fmt.Printf"));
    assert!(fixtures::GO_NEGATIVE.contains("logger.Info"));
}
