use std::sync::Arc;

use axum::{middleware, Router};
use axum::http::{header, HeaderName, HeaderValue};
use secrecy::ExposeSecret;
use sqlx::PgPool;
use tower_http::{
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
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
        crate::integrations::stripe::StripeClient::new(self.cfg.stripe_secret_key.expose_secret(), &self.http)
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
        .merge(crate::domain::payments::routes::router())
        .merge(crate::domain::popups::routes::router())
        .merge(crate::domain::portal::routes::router())
        .merge(crate::domain::admin::routes::router())
        .merge(crate::domain::tokens::routes::router())
        .merge(crate::domain::tournaments::routes::router())

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

        // ── Security headers ──────────────────────────────────────────────
        // Applied at the outermost layer so every response carries them.
        .layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-permitted-cross-domain-policies"),
            HeaderValue::from_static("none"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))

        // ── State ─────────────────────────────────────────────────────────
        .with_state(state)
}
