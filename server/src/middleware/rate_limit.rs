use axum::{
    extract::{ConnectInfo, Extension, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_REQUESTS: usize = 120;
const WINDOW_SECS:  u64   = 60;

pub struct RateLimiter {
    windows: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { windows: Mutex::new(HashMap::new()) })
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let now    = Instant::now();
        let window = Duration::from_secs(WINDOW_SECS);
        let mut map = self.windows.lock().unwrap();
        let deque = map.entry(ip).or_insert_with(VecDeque::new);
        deque.retain(|&t| now.duration_since(t) < window);
        if deque.len() >= MAX_REQUESTS { return false; }
        deque.push_back(now);
        true
    }
}

pub type SharedRateLimiter = Arc<RateLimiter>;

// Prefer X-Forwarded-For (Railway terminates TLS at the proxy layer).
fn client_ip(req: &Request) -> IpAddr {
    if let Some(xff) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = xff.to_str() {
            if let Ok(ip) = s.split(',').next().unwrap_or("").trim().parse() {
                return ip;
            }
        }
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

pub async fn check(
    Extension(limiter): Extension<SharedRateLimiter>,
    req:  Request,
    next: Next,
) -> Response {
    if !limiter.allow(client_ip(&req)) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "rate limited" }))).into_response();
    }
    next.run(req).await
}
