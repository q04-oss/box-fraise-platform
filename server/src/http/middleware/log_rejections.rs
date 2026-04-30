/// Middleware that emits a structured warning on every 401 or 403 response.
///
/// Captures method, path, IP, and user_id (decoded from the Bearer token if
/// present — no Redis revocation check, this is for logging only) so every
/// auth rejection is queryable in production without touching individual handlers.
///
/// Position in the stack: outer of hmac + rate_limit so it also captures
/// rejections from those layers, not just from route handlers.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::{app::AppState, http::middleware::rate_limit::client_ip};

pub async fn log_rejections(
    State(state):      State<AppState>,
    req:               Request,
    next:              Next,
) -> Response {
    let method  = req.method().as_str().to_owned();
    let path    = req.uri().path().to_owned();
    let ip: IpAddr = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| client_ip(req.headers(), Some(c)))
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    // Decode user_id from Bearer token for log context only.
    // verify_token checks the JWT signature; revocation is intentionally skipped
    // here — a revoked token still identifies the account for forensics.
    let user_id: Option<i32> = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| crate::auth::verify_token(token, &state.cfg))
        .map(|claims| i32::from(claims.user_id));

    let response = next.run(req).await;

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        tracing::warn!(
            method  = %method,
            path    = %path,
            ip      = %ip,
            user_id = ?user_id,
            status  = status.as_u16(),
            "auth rejection"
        );
    }

    response
}
