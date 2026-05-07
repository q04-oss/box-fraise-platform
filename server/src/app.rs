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
use box_fraise_integrations::storage::StorageClient;
use axum_prometheus::PrometheusMetricLayer;
use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::OnceLock;

/// The `metrics-exporter-prometheus` crate registers a global recorder; the
/// pair must therefore be constructed exactly once per process. Tests run
/// under one process and each build their own `AppState`, so we memoise the
/// pair here and clone both pieces out of the `OnceLock` for every caller.
/// Public so test fixtures can pull from the same memoised pair — that
/// guarantees the handle in `AppState` and the layer registered with the
/// router both observe the same recorder.
pub fn prometheus_pair() -> (PrometheusMetricLayer<'static>, PrometheusHandle) {
    static PAIR: OnceLock<(PrometheusMetricLayer<'static>, PrometheusHandle)> = OnceLock::new();
    PAIR.get_or_init(PrometheusMetricLayer::pair).clone()
}
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
    /// DigitalOcean Spaces storage client (Hardening Section 3). `None` when
    /// the `SPACES_*` env vars aren't set — handlers that need storage must
    /// return `503` in that case.
    pub storage_client:   Option<Arc<StorageClient>>,
    /// Prometheus metric handle (Hardening §4). The same handle that the
    /// metrics layer registered with — call `.render()` from `/metrics`.
    pub metric_handle:    PrometheusHandle,
}

impl AppState {
    pub fn new(
        db:             PgPool,
        cfg:            Config,
        storage_client: Option<Arc<StorageClient>>,
    ) -> Self {
        let (_layer, metric_handle) = prometheus_pair();
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
            storage_client,
            metric_handle,
        }
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

#[allow(deprecated)] // tower_http 0.6 deprecated TimeoutLayer::new; no non-deprecated replacement yet
pub fn build(state: AppState) -> Router {
    let (prometheus_layer, _handle) = prometheus_pair();

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
        .merge(crate::domain::analytics::routes::router())
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
        //
        // Per-route timeout intentions (Hardening §6 — TODO):
        //   - POST /api/identity/webhook/stripe          → 60s (Stripe slow)
        //   - POST /api/background-checks/webhook        → 60s
        //   - POST /api/dorotka/ask                      → 120s (LLM)
        //   - everything else                             → 30s (current global)
        // Implementing per-route timeouts requires wrapping the affected
        // routes individually with a `TimeoutLayer` of a different value;
        // deferred to a follow-up — the global 30s is the immediate win.
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        // ── Observability (Hardening §4) ──────────────────────────────────────
        // Prometheus middleware records http_requests_total,
        // http_request_duration_seconds, and http_requests_in_flight per
        // route automatically. Wire OUTSIDE timeout/correlation_id so that
        // metrics observe what reached the wire, not the inner-handler view.
        .layer(prometheus_layer)
        //
        // NOTE: Sentry's tower middleware (sentry-tower 0.34) is intentionally
        // omitted here — its `SentryLayer<_, _, Request<Body>>` is not `Sync`
        // under axum 0.8 (axum's `Body` itself isn't `Sync`), so it doesn't
        // satisfy `Router::layer`'s `Layer + Sync + 'static` bound. The
        // sentry-tracing layer in `lib.rs::run` already captures every
        // `tracing::error!` event into Sentry, which is the main value;
        // request-transaction wrapping is the only thing we lose. Revisit
        // when sentry-tower releases an axum-0.8-compatible variant.
        // ── Transport ─────────────────────────────────────────────────────────
        .layer(CompressionLayer::new())
        // ── CORS lockdown (Hardening §6) ─────────────────────────────────────
        // Origins: explicit allow-list from `cfg.allowed_origins`
        //   (set via ALLOWED_ORIGINS, comma-separated; default
        //   http://localhost:3000 in dev).
        // Methods: GET, POST, PATCH, DELETE, OPTIONS.
        // Headers: Authorization, Content-Type, Accept, X-Request-Id.
        // Credentials: true (paired with explicit origins — wildcard +
        //   credentials is forbidden by spec).
        // Max-age: 3600 s — caches the preflight result for an hour.
        // Exposed headers: X-Request-Id (correlation ID for client tracing).
        .layer({
            use axum::http::{HeaderName, Method};
            use tower_http::cors::AllowOrigin;
            let origins: Vec<axum::http::HeaderValue> = state
                .cfg
                .allowed_origins
                .iter()
                .filter_map(|o| axum::http::HeaderValue::from_str(o).ok())
                .collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_methods([
                    Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::ACCEPT,
                    HeaderName::from_static("x-request-id"),
                ])
                .allow_credentials(true)
                .max_age(std::time::Duration::from_secs(3600))
                .expose_headers([HeaderName::from_static("x-request-id")])
        })
        // ── Security headers ──────────────────────────────────────────────────
        // HSTS + X-Permitted-Cross-Domain-Policies stay static — neither
        // varies per request and they predate Hardening §6.
        .layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-permitted-cross-domain-policies"),
            HeaderValue::from_static("none"),
        ))
        // Per-response security headers (Hardening §6) — CSP nonce, plus
        // X-Content-Type-Options, X-Frame-Options, Referrer-Policy, and
        // Permissions-Policy. The middleware runs AFTER the inner pipeline
        // (request → response) so its headers stick to the final response.
        .layer(middleware::from_fn(crate::http::middleware::security_headers::apply))
        // ── State ─────────────────────────────────────────────────────────────
        .with_state(state)
}
