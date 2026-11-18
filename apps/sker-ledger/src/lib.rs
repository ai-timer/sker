pub mod health;
pub mod metrics;
pub mod metrics_store;
pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub bearer_token: Option<String>,
}
