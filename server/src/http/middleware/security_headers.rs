//! Per-response security headers — Hardening §6.
//!
//! Owns the headers whose value either varies per request (CSP carries a
//! fresh nonce so inline `<script>`/`<style>` are bound to that exact
//! response) or whose value the policy spec calls for here:
//!
//! - `Content-Security-Policy`  (per-request nonce)
//! - `X-Content-Type-Options`   (static `nosniff`)
//! - `X-Frame-Options`          (static `DENY`)
//! - `Referrer-Policy`          (static `strict-origin-when-cross-origin`)
//! - `Permissions-Policy`       (static `geolocation=(), microphone=(), camera=()`)
//!
//! HSTS and `X-Permitted-Cross-Domain-Policies` are still set as static
//! `SetResponseHeaderLayer`s in `app.rs` — they don't need per-request
//! variation and predate this middleware.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue},
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use rand::RngCore;

/// Attach the per-response security headers. Generates a 16-byte CSP nonce
/// from `OsRng` for every request — never reuse one across responses.
pub async fn apply(req: Request<Body>, next: Next) -> Response {
    let mut nonce_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = STANDARD_NO_PAD.encode(nonce_bytes);

    let csp = format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}'; \
         style-src 'self' 'nonce-{nonce}'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         frame-ancestors 'none'",
    );

    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    if let Ok(v) = HeaderValue::from_str(&csp) {
        headers.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::X_FRAME_OPTIONS,        HeaderValue::from_static("DENY"));
    headers.insert(header::REFERRER_POLICY,        HeaderValue::from_static("strict-origin-when-cross-origin"));
    headers.insert(
        axum::http::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );

    response
}
