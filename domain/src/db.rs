use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

/// Create and return a Postgres connection pool.
///
/// Pool is configured for the typical Axum single-binary deployment:
/// - max 20 connections (headroom for concurrent requests)
/// - min 2 connections (warm connections at startup)
/// - 5-second acquire timeout (fail fast rather than queue indefinitely)
/// - 10-minute idle timeout and 30-minute max lifetime (recycle behind PgBouncer)
pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect(url)
        .await
        .map_err(Into::into)
}
