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
    crypto::Ed25519KeyPair,
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
    /// Ed25519 key pair for soultoken signing (BFIP cryptography.md Section 4 /
    /// Hardening Section 1b). Wrapped in `Arc` because `Ed25519KeyPair` is not
    /// `Clone` and `AppState` is cloned per request.
    pub ed25519_key_pair: Arc<Ed25519KeyPair>,
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

        // Load the Ed25519 key pair from SOULTOKEN_SIGNING_KEY_HEX. Failure is
        // fatal — the server must not start with an unsignable soultoken path.
        let ed25519_key_pair = Ed25519KeyPair::from_hex(
            cfg.soultoken_signing_key_hex.expose_secret(),
        )
        .unwrap_or_else(|e| {
            tracing::error!(error = ?e, "SOULTOKEN_SIGNING_KEY_HEX could not be loaded");
            eprintln!("FATAL: SOULTOKEN_SIGNING_KEY_HEX is invalid: {e:?}");
            std::process::exit(1);
        });

        let derived_pub = ed25519_key_pair.verifying_key_hex();
        if !derived_pub.eq_ignore_ascii_case(&cfg.soultoken_verifying_key_hex) {
            tracing::error!(
                derived = %derived_pub,
                configured = %cfg.soultoken_verifying_key_hex,
                "SOULTOKEN_VERIFYING_KEY_HEX does not match the public key derived \
                 from SOULTOKEN_SIGNING_KEY_HEX",
            );
            eprintln!(
                "FATAL: SOULTOKEN_VERIFYING_KEY_HEX does not match the public key \
                 derived from the signing key. Update the env var to: {derived_pub}",
            );
            std::process::exit(1);
        }

        tracing::info!(
            verifying_key_hex = %derived_pub,
            "Ed25519 soultoken signing key loaded",
        );

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
            ed25519_key_pair: Arc::new(ed25519_key_pair),
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
        .merge(crate::domain::orders::routes::router())
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
        .merge(crate::domain::support::routes::router())
        .merge(crate::domain::attestation_tokens::routes::router())
        .merge(crate::domain::verification_events::routes::router())
        .merge(crate::domain::platform_configuration::routes::router())
        // ── Security middleware (innermost — runs first) ───────────────────────
        //
        // TODO(rls-enforcement, Hardening 2d): wire `set_rls_user_context`
        // (domain/src/db.rs) into the request lifecycle. A naive middleware
        // that calls `set_config('app.user_id', ..., true)` on a freshly
        // acquired pool connection is unsafe — outside an explicit
        // transaction, Postgres downgrades the setting to session scope and
        // the next request to pick up the same pooled connection inherits
        // the previous user's context. The correct shape is per-request
        // transactions: `pool.begin()` once, set the RLS context on that
        // connection, run all handler queries on the same connection,
        // commit/rollback at request end. That's a service-layer refactor
        // (every `&PgPool` parameter becomes `&mut PgConnection` or similar)
        // and is deferred until the transaction-per-request scaffolding
        // lands. Until then, RLS enforcement only applies when
        // APP_USER_DATABASE_URL is set AND the application explicitly opens
        // a transaction before calling RLS-protected paths.
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
