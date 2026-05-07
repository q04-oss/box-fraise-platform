/// Per-IP rate limiter — 120 requests per 60-second fixed window.
///
/// When Redis is configured, uses INCR + EXPIRE (cross-instance, consistent
/// with all other rate limits in the codebase). Falls back to an in-process
/// sliding-window HashMap when Redis is absent (single-instance only).
///
/// IP resolution: X-Forwarded-For first (Railway proxy), then socket peer address.
///
/// Hardening §6 / cleanup #8 — intended per-endpoint, per-user rate limits.
///
/// **Status**: limit *values* are seeded into `platform_configuration` by
/// migration `009_rate_limits.sql` so they're operator-tunable without a
/// redeploy. The middleware that consumes them is **not yet wired** —
/// see the architectural note below.
///
/// | Route                                   | Limit            | platform_configuration key                |
/// |-----------------------------------------|------------------|-------------------------------------------|
/// | POST /api/attestations                  | 10 / hour / user | `rate_limit_attestations_per_hour`        |
/// | POST /api/background-checks/initiate    | 5 / day / user   | `rate_limit_background_checks_per_day`    |
/// | POST /api/identity/initiate             | 3 / day / user   | `rate_limit_identity_initiations_per_day` |
/// | POST /api/dorotka/ask                   | 20 / hour / user | `rate_limit_dorotka_per_hour`             |
/// | POST /api/auth/magic-link/request       | 5 / hour / email | (not yet seeded — pre-auth bucket)        |
/// | All other routes                        | 120 / min / IP   | hard-coded `MAX_REQUESTS` below            |
///
/// **Architectural note (deferred)**: this middleware runs *before* JWT
/// validation in the stack at `server/src/app.rs`, so it cannot key on
/// `user_id` today — only on `IpAddr`. Implementing per-user limits
/// requires either (a) a second middleware that runs *after* the auth
/// extractor and reads the per-route limit from `platform_configuration`,
/// or (b) per-route extractors that do the rate check inline. Both are
/// non-trivial — they need the same Redis-backed `INCR + EXPIRE` shape
/// already used for the global IP bucket below, but keyed by `(user_id,
/// route)` and TTL'd to the config-driven window.
///
/// TODO(hardening): wire post-auth per-user middleware that reads the
/// `rate_limit_*` config rows and applies them. Until then, the global
/// IP bucket and the existing `dorotka_rate` (per-IP, in `AppState`) are
/// the only enforcement.
use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use deadpool_redis::redis;
use serde_json::json;

use crate::app::AppState;

const MAX_REQUESTS: i64 = 120;
const WINDOW_SECS:  u64 = 60;

// ── In-process fallback limiter ───────────────────────────────────────────────

pub struct RateLimiter {
    windows:      Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    max_requests: usize,
    window:       Duration,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            windows: Mutex::new(HashMap::new()),
            max_requests,
            window: Duration::from_secs(window_secs),
        })
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut map = self.windows.lock().unwrap();
        let deque = map.entry(ip).or_default();
        deque.retain(|&t| now.duration_since(t) < self.window);
        if deque.len() >= self.max_requests {
            return false;
        }
        deque.push_back(now);
        true
    }
}

pub type SharedRateLimiter = Arc<RateLimiter>;

// ── Middleware ────────────────────────────────────────────────────────────────

pub async fn check(
    State(state): State<AppState>,
    req:          Request,
    next:         Next,
) -> Response {
    let ip      = client_ip(req.headers(), req.extensions().get::<ConnectInfo<SocketAddr>>());
    let allowed = if let Some(pool) = &state.redis {
        redis_allow(pool, ip, &state.rate).await
    } else {
        state.rate.allow(ip)
    };

    if !allowed {
        // Hardening §6 — Retry-After tells well-behaved clients exactly when
        // to retry instead of beating on the limiter with exponential backoff.
        // Value matches the fixed-window length above.
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, WINDOW_SECS.to_string())],
            Json(json!({ "error": "rate_limited", "message": "rate limit exceeded" })),
        )
            .into_response();
    }
    next.run(req).await
}

/// Redis-backed rate check: INCR fraise:rate:ip:{ip} EX 60.
/// Returns true if the request is within the limit, false if exceeded.
/// On Redis failure, falls back to the in-process limiter rather than failing open.
async fn redis_allow(pool: &deadpool_redis::Pool, ip: IpAddr, fallback: &SharedRateLimiter) -> bool {
    let mut conn = match pool.get().await {
        Ok(c)  => c,
        Err(e) => {
            tracing::warn!(error = %e, "rate limit Redis pool error — using in-process fallback");
            return fallback.allow(ip);
        }
    };

    let key = format!("fraise:rate:ip:{ip}");
    let count: i64 = match redis::cmd("INCR").arg(&key).query_async(&mut *conn).await {
        Ok(n)  => n,
        Err(e) => {
            tracing::warn!(error = %e, "rate limit INCR failed — using in-process fallback");
            return fallback.allow(ip);
        }
    };

    if count == 1 {
        // First request in this window — set the expiry.
        let _: () = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(WINDOW_SECS)
            .query_async(&mut *conn)
            .await
            .unwrap_or(());
    }

    count <= MAX_REQUESTS
}

// ── IP resolution ─────────────────────────────────────────────────────────────

pub fn client_ip(headers: &HeaderMap, connect: Option<&ConnectInfo<SocketAddr>>) -> IpAddr {
    // X-Forwarded-For: client, proxy1, proxy2 — take the leftmost.
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(s) = xff.to_str() {
            if let Some(first) = s.split(',').next() {
                if let Ok(ip) = first.trim().parse() {
                    return ip;
                }
            }
        }
    }
    connect
        .map(|c| c.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}
