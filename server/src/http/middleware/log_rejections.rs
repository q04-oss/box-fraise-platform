/// Middleware that emits a structured warning on every 401 or 403 response.
///
/// method, path, and request_id come from the correlation_id span that wraps
/// this middleware — they appear automatically in every tracing event here.
/// This handler only adds the unique-to-rejection fields: ip and user_id.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::{app::AppState, http::middleware::rate_limit::client_ip};

pub async fn log_rejections(
    State(state): State<AppState>,
    req:          Request,
    next:         Next,
) -> Response {
    let ip: IpAddr = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| client_ip(req.headers(), Some(c)))
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    // Decode user_id from Bearer token for log context only.
    // verify_token checks the JWT signature; revocation is intentionally skipped —
    // a revoked token still identifies the account for forensics.
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
        // method, path, request_id come from the enclosing correlation_id span.
        tracing::warn!(ip = %ip, user_id = ?user_id, status = status.as_u16(), "auth rejection");
    }

    response
}
