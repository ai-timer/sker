use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{AppState, health, metrics, metrics_store};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/events", post(ingest_events))
        .route("/api/v1/metrics/overview", get(metrics_overview))
        .route("/api/v1/rules/health", get(rules_health))
}

pub async fn dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

fn authorized(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(ref expected) = state.bearer_token else {
        return false;
    };
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| h.strip_prefix("Bearer ").map(str::trim) == Some(expected.trim()))
}
fn error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"code":code,"message":message})))
}

async fn healthz(State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_err()
    {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "database is unavailable",
        ));
    }
    Ok(Json(json!({"status":"ready","database":"connected"})))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventBatch {
    schema_version: String,
    events: Vec<Value>,
}
#[derive(Debug, Serialize)]
struct Receipt {
    status: &'static str,
    accepted_event_ids: Vec<String>,
    duplicate_event_ids: Vec<String>,
}

async fn ingest_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(batch): Json<EventBatch>,
) -> Result<(StatusCode, Json<Receipt>), (StatusCode, Json<Value>)> {
    if !authorized(&headers, &state) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        ));
    }
    if batch.schema_version != "sker.ledger-events/v1"
        || batch.events.is_empty()
        || batch.events.len() > 500
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_batch",
            "schema_version must be sker.ledger-events/v1 and events must contain 1..=500 items",
        ));
    }
    let mut accepted = Vec::new();
    let mut duplicates = Vec::new();
    let mut tx = state.pool.begin().await.map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "database is unavailable",
        )
    })?;
    for event in batch.events {
        validate_event(&event)?;
        let id = event["event_id"].as_str().unwrap().to_owned();
        let result=sqlx::query("INSERT INTO governance_events(event_id, occurred_at, event_type, rule_id, bundle_sha256, track, state, payload) VALUES ($1::uuid,$2::timestamptz,$3,$4,$5,$6,$7,$8) ON CONFLICT(event_id) DO NOTHING")
            .bind(&id).bind(event["occurred_at"].as_str().unwrap()).bind(event["event_type"].as_str().unwrap())
            .bind(event.get("rule_id").and_then(Value::as_str)).bind(event["bundle_sha256"].as_str().unwrap())
            .bind(event.get("track").and_then(Value::as_str)).bind(event.get("state").and_then(Value::as_str)).bind(&event)
            .execute(&mut *tx).await.map_err(|_|error(StatusCode::SERVICE_UNAVAILABLE,"unavailable","event insert failed"))?;
        if result.rows_affected() == 1 {
            accepted.push(id)
        } else {
            duplicates.push(id)
        }

        if let Some(lifecycles) = event.get("lifecycles").and_then(Value::as_array) {
            for lc in lifecycles {
                if let Some(rule_id) = lc.get("rule_id").and_then(Value::as_str) {
                    let cooldown = lc
                        .get("cooldown_hours")
                        .and_then(Value::as_i64)
                        .map(|v| v as i32);
                    let hotfix_exp = lc.get("hotfix_expires_at").and_then(Value::as_str);
                    let review_by = lc.get("review_by").and_then(Value::as_str);
                    let _ = sqlx::query(
                        r#"
                        INSERT INTO rule_lifecycles(rule_id, activated_at, cooldown_hours, hotfix_expires_at, review_by, updated_at)
                        VALUES ($1, now(), $2, $3::timestamptz, $4::timestamptz, now())
                        ON CONFLICT(rule_id) DO UPDATE SET
                            cooldown_hours = EXCLUDED.cooldown_hours,
                            hotfix_expires_at = EXCLUDED.hotfix_expires_at,
                            review_by = EXCLUDED.review_by,
                            updated_at = now()
                        "#
                    )
                    .bind(rule_id)
                    .bind(cooldown)
                    .bind(hotfix_exp)
                    .bind(review_by)
                    .execute(&mut *tx)
                    .await;
                }
            }
        }
    }
    tx.commit().await.map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "event commit failed",
        )
    })?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Receipt {
            status: "accepted",
            accepted_event_ids: accepted,
            duplicate_event_ids: duplicates,
        }),
    ))
}

pub fn validate_event(e: &Value) -> Result<(), (StatusCode, Json<Value>)> {
    let fail = || {
        error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_event",
            "event does not match the sanitized governance-event contract",
        )
    };
    let o = e.as_object().ok_or_else(fail)?;
    let allowed = [
        "event_id",
        "occurred_at",
        "event_type",
        "rule_id",
        "bundle_sha256",
        "track",
        "state",
        "counters",
        "labels",
        "lifecycles",
    ];
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(fail());
    }
    if serde_json::from_value::<sker_schema::GovernanceEvent>(e.clone()).is_err() {
        return Err(fail());
    }
    let uuid = o.get("event_id").and_then(Value::as_str).ok_or_else(fail)?;
    if uuid.len() != 36
        || !uuid.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(fail());
    }
    let at = o
        .get("occurred_at")
        .and_then(Value::as_str)
        .ok_or_else(fail)?;
    if sqlx::types::chrono::DateTime::parse_from_rfc3339(at).is_err() {
        return Err(fail());
    }
    if ![
        "consult", "search", "check", "attest", "proposal", "ci_gate", "replay", "compile",
    ]
    .contains(
        &o.get("event_type")
            .and_then(Value::as_str)
            .ok_or_else(fail)?,
    ) {
        return Err(fail());
    }
    let hash = o
        .get("bundle_sha256")
        .and_then(Value::as_str)
        .ok_or_else(fail)?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(fail());
    }
    if let Some(r) = o.get("rule_id")
        && !r.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128)
    {
        return Err(fail());
    }
    if let Some(s) = o.get("state")
        && !["comply", "not_triggered_in_logic", "exempt", "escalate"]
            .contains(&s.as_str().unwrap_or(""))
    {
        return Err(fail());
    }
    // Free text and identity-bearing keys are rejected at every nested level.
    fn privacy(v: &Value) -> bool {
        match v {
            Value::Object(m) => m.iter().any(|(k, v)| {
                [
                    "source",
                    "source_code",
                    "prompt",
                    "secret",
                    "email",
                    "user_id",
                    "identity",
                ]
                .contains(&k.as_str())
                    || privacy(v)
            }),
            Value::Array(a) => a.iter().any(privacy),
            Value::String(s) => s.contains("-----BEGIN ") || s.contains("@"),
            _ => false,
        }
    }
    if privacy(e) {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "privacy_violation",
            "event contains a prohibited source, secret, prompt, or personal identifier",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct WindowQuery {
    window: Option<String>,
    from: Option<String>,
    to: Option<String>,
}
async fn metrics_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<WindowQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !authorized(&headers, &state) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        ));
    }
    let window = metrics::parse_window(q.window.as_deref(), q.from.as_deref(), q.to.as_deref())
        .map_err(|m| error(StatusCode::BAD_REQUEST, "invalid_window", m))?;
    metrics_store::overview(&state.pool, window)
        .await
        .map(Json)
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "metrics unavailable",
            )
        })
}
#[derive(Debug, Deserialize)]
pub struct HealthQuery {
    window: Option<String>,
    status: Option<Vec<String>>,
}
async fn rules_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HealthQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !authorized(&headers, &state) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        ));
    }
    let window = health::parse_window(q.window.as_deref()).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_window",
            "window must be 7d, 30d, or 90d",
        )
    })?;
    if let Some(ref statuses) = q.status
        && statuses.iter().any(|s| {
            ![
                "suspected_dead",
                "noisy",
                "lifecycle_expiring",
                "lifecycle_expired",
            ]
            .contains(&s.as_str())
        })
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_status",
            "unknown health status",
        ));
    }
    metrics_store::rule_health(&state.pool, window, q.status)
        .await
        .map(Json)
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "rule health unavailable",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_event_contract_and_rejects_privacy_fields() {
        let good = json!({"event_id":"550e8400-e29b-41d4-a716-446655440000","occurred_at":"2026-11-18T00:00:00Z","event_type":"check","bundle_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"});
        assert!(validate_event(&good).is_ok());
        let bad = json!({"event_id":"550e8400-e29b-41d4-a716-446655440000","occurred_at":"2026-11-18T00:00:00Z","event_type":"check","bundle_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","prompt":"hello"});
        assert!(validate_event(&bad).is_err());
    }
    #[tokio::test]
    async fn health_route_router_builds_without_database() {
        let app = router();
        let _ = app;
    }
}
