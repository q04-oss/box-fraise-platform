/// Per-IP rate limiter — 120 requests per 60-second fixed window.
///
/// When Redis is configured, uses INCR + EXPIRE (cross-instance, consistent
/// with all other rate limits in the codebase). Falls back to an in-process
/// sliding-window HashMap when Redis is absent (single-instance only).
///
/// IP resolution: X-Forwarded-For first (Railway proxy), then socket peer address.
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
    windows: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            windows: Mutex::new(HashMap::new()),
        })
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let now    = Instant::now();
        let window = Duration::from_secs(WINDOW_SECS);
        let mut map = self.windows.lock().unwrap();
        let deque = map.entry(ip).or_insert_with(VecDeque::new);
        deque.retain(|&t| now.duration_since(t) < window);
        if deque.len() >= MAX_REQUESTS as usize {
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
        redis_allow(pool, ip).await
    } else {
        state.rate.allow(ip)
    };

    if !allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate limited" })),
        )
            .into_response();
    }
    next.run(req).await
}

/// Redis-backed rate check: INCR fraise:rate:ip:{ip} EX 60.
/// Returns true if the request is within the limit, false if exceeded.
/// On Redis failure, fails open (allows the request) and logs a warning.
async fn redis_allow(pool: &deadpool_redis::Pool, ip: IpAddr) -> bool {
    let mut conn = match pool.get().await {
        Ok(c)  => c,
        Err(e) => {
            tracing::warn!(error = %e, "rate limit Redis pool error — failing open");
            return true;
        }
    };

    let key = format!("fraise:rate:ip:{ip}");
    let count: i64 = match redis::cmd("INCR").arg(&key).query_async(&mut *conn).await {
        Ok(n)  => n,
        Err(e) => {
            tracing::warn!(error = %e, "rate limit INCR failed — failing open");
            return true;
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

fn client_ip(headers: &HeaderMap, connect: Option<&ConnectInfo<SocketAddr>>) -> IpAddr {
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
