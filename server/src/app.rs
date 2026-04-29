use std::sync::Arc;

use axum::{middleware, Router};
use sqlx::PgPool;
use tower_http::{
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use crate::{
    auth::{new_revoked_tokens, RevokedTokens},
    config::Config,
    http::{
        middleware::{
            hmac::{new_nonce_cache, NonceCache},
            rate_limit::{RateLimiter, SharedRateLimiter},
        },
        routes::meta,
    },
};

// ── AppState ──────────────────────────────────────────────────────────────────

/// Shared application state — cheap to clone (all heap data is Arc-backed).
#[derive(Clone)]
pub struct AppState {
    pub db:      PgPool,
    pub cfg:     Arc<Config>,
    pub revoked: RevokedTokens,
    pub nonces:  NonceCache,
    pub rate:    SharedRateLimiter,
    /// Shared HTTP client — reuses the connection pool across all integration calls.
    pub http:    reqwest::Client,
}

impl AppState {
    /// Borrow a Stripe client scoped to this request. Cheap — shares the
    /// underlying reqwest connection pool.
    pub fn stripe(&self) -> crate::integrations::stripe::StripeClient<'_> {
        crate::integrations::stripe::StripeClient::new(&self.cfg.stripe_secret_key, &self.http)
    }

    pub fn new(db: PgPool, cfg: Config) -> Self {
        Self {
            db,
            cfg:     Arc::new(cfg),
            revoked: new_revoked_tokens(),
            nonces:  new_nonce_cache(),
            rate:    RateLimiter::new(),
            http:    reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client is infallible"),
        }
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn build(state: AppState) -> Router {
    Router::new()
        // ── Platform-level routes ─────────────────────────────────────────
        .merge(meta::router())
        // ── Domain routes ─────────────────────────────────────────────────
        .merge(crate::domain::auth::routes::router())
        .merge(crate::domain::keys::routes::router())
        .merge(crate::domain::devices::routes::router())
        .merge(crate::domain::catalog::routes::router())
        .merge(crate::domain::orders::routes::router())
        .merge(crate::domain::messages::routes::router())
        .merge(crate::domain::users::routes::router())
        .merge(crate::domain::businesses::routes::router())
        .merge(crate::domain::memberships::routes::router())
        .merge(crate::domain::search::routes::router())
        .merge(crate::domain::campaigns::routes::router())
        .merge(crate::domain::contracts::routes::router())
        .merge(crate::domain::nfc::routes::router())
        .merge(crate::domain::gifts::routes::router())
        .merge(crate::domain::art::routes::router())
        .merge(crate::domain::ventures::routes::router())

        // ── Security middleware (innermost — applied last, runs first) ─────
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::hmac::validate,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::rate_limit::check,
        ))

        // ── Observability ─────────────────────────────────────────────────
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))

        // ── Transport ─────────────────────────────────────────────────────
        .layer(CompressionLayer::new())

        // ── State ─────────────────────────────────────────────────────────
        .with_state(state)
}
