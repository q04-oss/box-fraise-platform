/// Per-IP rate limiter — 120 requests per 60-second fixed window.
///
/// When Redis is configured, uses INCR + EXPIRE (cross-instance, consistent
/// with all other rate limits in the codebase). Falls back to an in-process
/// sliding-window HashMap when Redis is absent (single-instance only).
///
/// IP resolution: X-Forwarded-For first (Railway proxy), then socket peer address.
///
/// Hardening §6 — intended per-endpoint rate limits (TODO: implement):
///   - POST /api/attestations                    → max 10  per hour  per user
///   - POST /api/background-checks/initiate      → max  5  per day   per user
///   - POST /api/identity/initiate               → max  3  per day   per user
///   - POST /api/dorotka/ask                     → max 20  per hour  per user
///   - POST /api/auth/magic-link/request         → max  5  per hour  per email
///   - All other routes                          → existing 120/min/IP global
/// TODO(hardening): implement per-route rate limits — likely a Redis-backed
/// keyed counter middleware that runs after JWT validation so the user_id /
/// email is available as the bucket key.
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
