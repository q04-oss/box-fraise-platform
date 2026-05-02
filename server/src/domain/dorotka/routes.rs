
use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::middleware::rate_limit::client_ip,
};
use super::service;

/// 4 KB body cap — queries are short text, anything larger is suspicious.
const BODY_LIMIT: usize = 4_096;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/dorotka/ask", post(ask))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_LIMIT))
}

#[derive(Deserialize)]
pub(crate) struct AskBody {
    query: String,
    // context is intentionally absent — derived server-side from the Host header
    // so callers cannot select a system prompt that doesn't belong to their surface.
}

#[derive(Serialize)]
pub(crate) struct AskResponse {
    answer:  String,
    context: String,
}

pub(crate) async fn ask(
    State(state):      State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers:           HeaderMap,
    Json(body):        Json<AskBody>,
) -> AppResult<Json<AskResponse>> {
    // Use X-Forwarded-For (set by Railway's proxy) — ConnectInfo gives the
    // proxy's IP, not the client's.
    let ip = client_ip(&headers, Some(&ConnectInfo(addr)));

    // Rate check before any other work — prevents wasted Anthropic spend
    rate_check(&state, ip)?;

    // Sanitise input — returns 400 for empty or oversized queries
    let query = service::sanitise(&body.query)
        .map_err(|e| AppError::bad_request(e.to_string()))?;

    let context = context_from_host(&headers);

    use secrecy::ExposeSecret;
    let api_key = state.cfg.anthropic_api_key.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "ANTHROPIC_API_KEY not configured — Dorotka unavailable"
        )))?;

    let base_url = state.cfg.anthropic_base_url.as_deref()
        .unwrap_or(box_fraise_integrations::anthropic::DEFAULT_API_URL);

    let answer = service::ask_dorotka(
        &state.db,
        &state.http,
        api_key.expose_secret(),
        base_url,
        &query,
        context,
        ip,
        &state.event_bus,
    ).await?;

    Ok(Json(AskResponse {
        answer,
        context: context.to_string(),
    }))
}

/// Derives the Dorotka context from the Host header.
/// "whisked.*" hostnames get the Whisked persona; everything else gets the platform voice.
/// This is intentionally server-side — callers must not select their own system prompt.
fn context_from_host(headers: &HeaderMap) -> &'static str {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if host.starts_with("whisked.") {
        "whisked"
    } else {
        "fraise"
    }
}

/// Sliding-window rate check — 20 req/IP/60 s.
/// Uses the same VecDeque<Instant> approach as the global SharedRateLimiter.
/// Always enforces the limit; never fails open.
fn rate_check(state: &AppState, ip: std::net::IpAddr) -> AppResult<()> {
    if state.dorotka_rate.allow(ip) {
        Ok(())
    } else {
        Err(AppError::TooManyRequests)
    }
}
