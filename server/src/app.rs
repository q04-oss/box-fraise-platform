use std::sync::Arc;

use axum::http::{header, HeaderName, HeaderValue};
use axum::{middleware, Router};
use deadpool_redis::Pool as RedisPool;
use sqlx::PgPool;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
};

use box_fraise_domain::{
    auth::{new_revoked_tokens, RevokedTokens},
    config::Config,
    event_bus::EventBus,
};
use crate::http::{
    middleware::{
        correlation_id,
        hmac::{new_nonce_cache, NonceCache},
        rate_limit::{RateLimiter, SharedRateLimiter},
    },
    routes::meta,
};

// ── AppState ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db:           PgPool,
    pub cfg:          Arc<Config>,
    pub revoked:      RevokedTokens,
    pub nonces:       NonceCache,
    pub redis:        Option<RedisPool>,
    pub rate:         SharedRateLimiter,
    pub dorotka_rate: SharedRateLimiter,
    pub http:         reqwest::Client,
    pub event_bus:    EventBus,
}

impl AppState {
    pub fn new(db: PgPool, cfg: Config) -> Self {
        use secrecy::ExposeSecret;

        let redis = cfg.redis_url.as_ref().and_then(|url| {
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
            cfg:          Arc::new(cfg),
            revoked:      new_revoked_tokens(),
            nonces:       new_nonce_cache(),
            redis,
            rate:         RateLimiter::new(120, 60),
            dorotka_rate: RateLimiter::new(20, 60),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client is infallible"),
            event_bus: EventBus::new(),
        }
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

#[allow(deprecated)] // tower_http 0.6 deprecated TimeoutLayer::new; no non-deprecated replacement yet
pub fn build(state: AppState) -> Router {
    Router::new()
        // ── OpenAPI docs ──────────────────────────────────────────────────────
        .merge(crate::openapi::router())
        // ── Domain routes ─────────────────────────────────────────────────────
        .merge(meta::router())
        .merge(crate::domain::attestations::routes::router())
        .merge(crate::domain::soultokens::routes::router())
        .merge(crate::domain::auth::routes::router())
        .merge(crate::domain::background_checks::routes::router())
        .merge(crate::domain::beacons::routes::router())
        .merge(crate::domain::businesses::routes::router())
        .merge(crate::domain::presence::routes::router())
        .merge(crate::domain::identity_credentials::routes::router())
        .merge(crate::domain::staff::routes::router())
        .merge(crate::domain::users::routes::router())
        .merge(crate::domain::dorotka::routes::router())
        // ── Security middleware (innermost — runs first) ───────────────────────
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::hmac::validate,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::rate_limit::check,
        ))
        // Outer of hmac + rate_limit; captures their 401/403 rejections.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::middleware::log_rejections::log_rejections,
        ))
        // Correlation ID: wraps everything above so every log line from every
        // handler includes request_id, method, path in its span context.
        .layer(middleware::from_fn(correlation_id::track))
        // Request timeout — returns 408 after 30 s. Inside correlation_id so
        // the request_id is available in timeout logs. Configurable via
        // TIMEOUT_SECS env var (default 30).
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        // ── Transport ─────────────────────────────────────────────────────────
        .layer(CompressionLayer::new())
        // CORS posture — review before production launch
        // Currently: permissive (allow all origins, no credentials)
        // Allowed origins: wildcard (*) — iOS native app does not send Origin;
        //   web clients (Swagger UI, future web app) use any origin
        // Credentials: not allowed (wildcard origin is incompatible with credentials)
        // Allowed methods: GET, POST, PATCH, PUT, DELETE, OPTIONS
        // Exposed headers: X-Request-Id (correlation ID for client-side tracing)
        // TODO: restrict to known iOS app origins before web app launch
        .layer(
            CorsLayer::permissive()
                .expose_headers([axum::http::HeaderName::from_static("x-request-id")]),
        )
        // ── Security headers ──────────────────────────────────────────────────
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
        // ── State ─────────────────────────────────────────────────────────────
        .with_state(state)
}
