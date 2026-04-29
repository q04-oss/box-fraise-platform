use axum::{middleware, routing::{get, post}, Extension, Router};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use crate::{
    auth::{new_revoked_tokens, RevokedTokens},
    middleware::{hmac, rate_limit::{self, SharedRateLimiter}},
};

pub fn build(pool: PgPool) -> Router {
    let nonce_cache   = hmac::new_nonce_cache();
    let revoked       = new_revoked_tokens();
    let rate_limiter  = rate_limit::RateLimiter::new();

    Router::new()
        .route("/health", get(crate::routes::health::get))
        .route("/api/auth/logout", post(crate::routes::auth::logout))
        // ── layers (innermost first) ─────────────────────────────────────
        .layer(middleware::from_fn(hmac::validate))
        .layer(Extension(pool))
        .layer(Extension(nonce_cache))
        .layer(Extension::<RevokedTokens>(revoked))
        .layer(middleware::from_fn(rate_limit::check))
        .layer(Extension::<SharedRateLimiter>(rate_limiter))
        .layer(TraceLayer::new_for_http())
}
