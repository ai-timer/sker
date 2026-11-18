use serde_json::json;

#[test]
fn event_contract_rejects_unknown_fields_and_non_allowlisted_values() {
    let mut event=json!({"event_id":"550e8400-e29b-41d4-a716-446655440000","occurred_at":"2026-11-18T00:00:00Z","event_type":"check","bundle_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"});
    assert!(sker_ledger::routes::validate_event(&event).is_ok());
    event["identity"] = json!("person@example.com");
    assert!(sker_ledger::routes::validate_event(&event).is_err());
}

#[test]
fn snr_uses_not_triggered_plus_override_and_strictly_exceeds_seventy_percent() {
    assert_eq!(sker_ledger::health::noise_rate(5, 2, 10), Some(0.7));
    assert!(!sker_ledger::health::noise_status(sker_ledger::health::noise_rate(5, 2, 10)));
    assert!(sker_ledger::health::noise_status(sker_ledger::health::noise_rate(6, 2, 10)));
}

#[tokio::test]
async fn postgres_migrations_and_queries_when_test_database_is_configured() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else { return; };
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
    for migration in [
        include_str!("../migrations/0001_events.sql"),
        include_str!("../migrations/0002_metric_views.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    sqlx::query("REFRESH MATERIALIZED VIEW ledger_daily_metrics").execute(&pool).await.unwrap();
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY ledger_daily_metrics").execute(&pool).await.unwrap();
    let now = sqlx::types::chrono::Utc::now();
    let _: serde_json::Value = sqlx::query_scalar(include_str!("../queries/overview.sql"))
        .bind(now - std::time::Duration::from_secs(7 * 86400)).bind(now).fetch_one(&pool).await.unwrap();
    let _ = sqlx::query(include_str!("../queries/rule_health.sql"))
        .bind(7_i64).fetch_all(&pool).await.unwrap();
}
