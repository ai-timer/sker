use anyhow::{Context, Result};
use axum::{Router, routing::get};
use sker_ledger::{AppState, routes};
use sqlx::postgres::PgPoolOptions;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn mask_database_url(url: &str) -> String {
    // Mask password: postgres://user:password@host:port/dbname -> postgres://user:********@host:port/dbname
    if let Some((scheme_and_auth, rest)) = url.split_once('@') {
        if let Some((prefix, _pass)) = scheme_and_auth.rsplit_once(':') {
            return format!("{prefix}:********@{rest}");
        }
        return format!("postgres://********@{rest}");
    }
    "postgres://[configured]".to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("============================================================");
    println!("  Sker Ledger v{VERSION} - Audit Log & Telemetry Server");
    println!("============================================================");

    let database_url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL environment variable is not set.\n\
         Example: DATABASE_URL=postgres://sker:sker@localhost:5432/sker_ledger\n\
         Please set it or copy apps/sker-ledger/.env.example to .env",
    )?;

    println!(
        "Connecting to PostgreSQL: {}",
        mask_database_url(&database_url)
    );

    let pool = match PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            println!("PostgreSQL connection established successfully.");
            pool
        }
        Err(err) => {
            eprintln!("\n[ERROR] Failed to connect to PostgreSQL database:");
            eprintln!("  Error detail: {err}");
            eprintln!("\n[Troubleshooting Tips]");
            eprintln!(
                "  1. Ensure Postgres service is running (e.g. `docker compose up -d postgres`)."
            );
            eprintln!("  2. Verify credentials, host, port, and database name in DATABASE_URL.");
            eprintln!(
                "  3. If running via Docker Compose, ensure the container name matches the host."
            );
            eprintln!("  4. Check network connectivity or firewall rules.\n");
            return Err(err).context("PostgreSQL connection failed");
        }
    };

    println!("Executing database migrations...");
    let migrations = [
        (
            "0001_events.sql",
            include_str!("../migrations/0001_events.sql"),
        ),
        (
            "0002_metric_views.sql",
            include_str!("../migrations/0002_metric_views.sql"),
        ),
    ];

    for (name, migration) in migrations {
        for statement in migration
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.starts_with("--"))
        {
            sqlx::raw_sql(statement)
                .execute(&pool)
                .await
                .with_context(|| format!("Migration error in statement from {name}"))?;
        }
        println!("  ✓ Applied migration: {name}");
    }

    sqlx::query("REFRESH MATERIALIZED VIEW ledger_daily_metrics")
        .execute(&pool)
        .await
        .context("Failed to refresh materialized view ledger_daily_metrics")?;
    println!("  ✓ Refreshed materialized view: ledger_daily_metrics");

    let bearer_token = std::env::var("LEDGER_BEARER_TOKEN").context(
        "LEDGER_BEARER_TOKEN environment variable is not set.\n\
         Example: LEDGER_BEARER_TOKEN=secret-telemetry-token\n\
         Required for authenticating incoming telemetry events.",
    )?;

    let state = AppState {
        pool,
        bearer_token: Some(bearer_token),
    };

    let app = Router::new()
        .route("/", get(routes::dashboard))
        .route("/dashboard", get(routes::dashboard))
        .merge(routes::router())
        .with_state(state);

    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("Failed to bind TCP listener to {listen_addr}"))?;

    let local_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or(listen_addr);

    println!("\nService is ready:");
    println!("  - HTTP Server:        http://{local_addr}");
    println!("  - Health Check:       http://{local_addr}/healthz");
    println!("  - Dashboard:          http://{local_addr}/dashboard");
    println!("  - Event Ingestion:    POST http://{local_addr}/api/v1/events");
    println!("  - Rule Health API:    GET  http://{local_addr}/api/v1/rules/health");
    println!("============================================================\n");

    axum::serve(listener, app).await?;
    Ok(())
}
