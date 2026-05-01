use std::sync::Arc;

use axum::http::{header, HeaderName, HeaderValue};
use axum::{middleware, Router};
use deadpool_redis::Pool as RedisPool;
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
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub revoked: RevokedTokens,
    /// In-process nonce cache — used as fallback when Redis is not configured.
    /// Safe for single-instance deployments only. Ignored when `redis` is Some.
    pub nonces: NonceCache,
    /// Redis pool for distributed nonce deduplication.
    /// None when REDIS_URL is not set — falls back to `nonces`.
    pub redis: Option<RedisPool>,
    pub rate: SharedRateLimiter,
    /// Separate sliding-window limiter for the Dorotka AI endpoint.
    /// 20 req/IP/min — tighter than the global limit because Anthropic calls have real cost.
    pub dorotka_rate: SharedRateLimiter,
    /// Shared HTTP client — reuses the connection pool across all integration calls.
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(db: PgPool, cfg: Config) -> Self {
        let redis = cfg.redis_url.as_ref().and_then(|url| {
            use secrecy::ExposeSecret;
            let url_str = url.expose_secret().to_owned();
            match deadpool_redis::Config::from_url(url_str)
                .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            {
                Ok(pool) => {
                    tracing::info!("Redis nonce cache configured");
                    Some(pool)
                }
                Err(e) => {
                    tracing::error!(error = %e, "Redis pool creation failed — check REDIS_URL");
                    None
                }
            }
        });

        if redis.is_none() {
            tracing::warn!(
                "REDIS_URL not configured — nonce cache is in-process. \
                 Safe for single instance only; set REDIS_URL before scaling."
            );
        }

        Self {
            db,
            cfg: Arc::new(cfg),
            revoked: new_revoked_tokens(),
            nonces: new_nonce_cache(),
            redis,
            rate:         RateLimiter::new(120, 60),
            dorotka_rate: RateLimiter::new(20, 60),
            http: reqwest::Client::builder()
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
        .merge(crate::domain::messages::routes::router())
        .merge(crate::domain::users::routes::router())
        .merge(crate::domain::dorotka::routes::router())
        // ── Security middleware (innermost — applied last, runs first) ─────
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::hmac::validate,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::rate_limit::check,
        ))
        // Outer of hmac + rate_limit so it sees their 401/403 rejections too.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::log_rejections::log_rejections,
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
            HeaderValue::from_static("geolocation=(), microphone=()"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; \
                 script-src 'self'; \
                 style-src 'self' 'unsafe-inline'; \
                 img-src 'self' data: blob:; \
                 connect-src 'self'; \
                 frame-ancestors 'none'",
            ),
        ))
        // ── State ─────────────────────────────────────────────────────────
        .with_state(state)
}
