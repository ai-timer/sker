use anyhow::Context;
use axum::{Router, routing::get};
use sqlx::postgres::PgPoolOptions;
use sker_ledger::{AppState, routes};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new().max_connections(10).connect(&database_url).await?;
    for migration in [include_str!("../migrations/0001_events.sql"), include_str!("../migrations/0002_metric_views.sql")] {
        for statement in migration.split(';').map(str::trim).filter(|s| !s.is_empty() && !s.starts_with("--")) {
            sqlx::raw_sql(statement).execute(&pool).await?;
        }
    }
    sqlx::query("REFRESH MATERIALIZED VIEW ledger_daily_metrics").execute(&pool).await?;
    let bearer_token = std::env::var("LEDGER_BEARER_TOKEN").context("LEDGER_BEARER_TOKEN must be set")?;
    let state = AppState { pool, bearer_token: Some(bearer_token) };
    let app = Router::new().route("/", get(routes::dashboard)).merge(routes::router()).with_state(state);
    let listener = tokio::net::TcpListener::bind(std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into())).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
