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
    audit,
    error::{AppError, AppResult},
    http::middleware::rate_limit::client_ip,
    integrations::anthropic,
};
use super::service;

/// 4 KB body cap — queries are short text, anything larger is suspicious.
const BODY_LIMIT: usize = 4_096;
/// 20 Dorotka requests per IP per minute.
/// Anthropic calls are expensive; this prevents runaway cost from a single source.
const RATE_LIMIT: i64 = 20;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/dorotka/ask", post(ask))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_LIMIT))
}

#[derive(Deserialize)]
struct AskBody {
    query: String,
    // context is intentionally absent — derived server-side from the Host header
    // so callers cannot select a system prompt that doesn't belong to their surface.
}

#[derive(Serialize)]
struct AskResponse {
    answer:  String,
    context: String,
}

async fn ask(
    State(state):      State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers:           HeaderMap,
    Json(body):        Json<AskBody>,
) -> AppResult<Json<AskResponse>> {
    // Use X-Forwarded-For (set by Railway's proxy) — ConnectInfo gives the
    // proxy's IP, not the client's.
    let ip = client_ip(&headers, Some(&ConnectInfo(addr)));

    // Rate check before any other work — cheap Redis call prevents wasted Anthropic spend
    rate_check(&state, ip).await?;

    // Sanitise input — returns 400 for empty or oversized queries
    let query = service::sanitise(&body.query)
        .map_err(|e| AppError::bad_request(&e.to_string()))?;

    let context = context_from_host(&headers);
    let system  = service::system_prompt(context);

    let api_key = state.cfg.anthropic_api_key.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "ANTHROPIC_API_KEY not configured — Dorotka unavailable"
        )))?;

    // Audit the attempt before the API call — records even if Anthropic fails
    audit::write(
        &state.db,
        None,
        None,
        "dorotka.ask",
        serde_json::json!({
            "context":       context,
            "query_preview": query.chars().take(80).collect::<String>(),
            "ip":            ip.to_string(),
        }),
        Some(ip),
    ).await;

    use secrecy::ExposeSecret;
    let answer = anthropic::ask(
        &state.http,
        api_key.expose_secret(),
        system,
        &query,
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

/// Fixed-window Redis rate limiter — same pattern as loyalty and HTML stamp endpoints.
async fn rate_check(state: &AppState, ip: std::net::IpAddr) -> AppResult<()> {
    use deadpool_redis::redis;

    // Fail closed — if Redis is unavailable, deny the request rather than
    // allowing unlimited Anthropic calls. A Redis outage is a plausible
    // attack precondition, not just an ops failure.
    let Some(pool) = state.redis.as_ref() else {
        return Err(AppError::Unprocessable("service temporarily unavailable".into()));
    };

    let key = format!("fraise:rate:dorotka:{ip}");
    let mut conn = pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis pool: {e}")))?;

    let count: i64 = redis::cmd("INCR")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;

    if count == 1 {
        // If EXPIRE fails the key has no TTL and will block this IP forever —
        // log the failure so it surfaces in ops rather than silently breaking.
        if let Err(e) = redis::cmd("EXPIRE")
            .arg(&key).arg(60u64)
            .query_async::<_, ()>(&mut *conn).await
        {
            tracing::error!(key = %key, error = %e, "dorotka rate limit EXPIRE failed — key has no TTL");
        }
    }

    if count > RATE_LIMIT {
        Err(AppError::Unprocessable("rate limit exceeded — try again shortly".into()))
    } else {
        Ok(())
    }
}
