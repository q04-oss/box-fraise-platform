use axum::{middleware, routing::get, Extension, Router};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use crate::middleware::hmac;

pub fn build(pool: PgPool) -> Router {
    let nonce_cache = hmac::new_nonce_cache();

    Router::new()
        .route("/health", get(crate::routes::health::get))
        // API routes go here as they're built
        .layer(middleware::from_fn(hmac::validate))
        .layer(Extension(pool))
        .layer(Extension(nonce_cache))
        .layer(TraceLayer::new_for_http())
}
