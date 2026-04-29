/// Per-IP sliding-window rate limiter.
///
/// Limits: 120 requests per 60-second window per IP address.
/// IP resolution: X-Forwarded-For first (Railway proxy), then socket peer address.
///
/// The window map grows unbounded for unique IPs; acceptable for a focused-audience
/// platform. Add periodic pruning or switch to a bounded LRU cache if open to the web.
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
use serde_json::json;

use crate::app::AppState;

const MAX_REQUESTS:  usize = 120;
const WINDOW_SECS:   u64   = 60;

// ── Limiter ───────────────────────────────────────────────────────────────────

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
        if deque.len() >= MAX_REQUESTS {
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
    let ip = client_ip(req.headers(), req.extensions().get::<ConnectInfo<SocketAddr>>());
    if !state.rate.allow(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate limited" })),
        )
            .into_response();
    }
    next.run(req).await
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
