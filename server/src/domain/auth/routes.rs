
use axum::{
    extract::{ConnectInfo, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use std::net::SocketAddr;

use crate::http::middleware::rate_limit::client_ip;

use super::{service, types::*};
use crate::{
    app::AppState,
    auth,
    error::{AppError, AppResult},
    http::extractors::{
        auth::{RequireClaims, RequireUser},
        json::AppJson,
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/apple",             post(apple))
        .route("/api/auth/me",                get(me))
        .route("/api/auth/push-token",        patch(push_token))
        .route("/api/auth/display-name",      patch(display_name))
        .route("/api/auth/logout",            post(logout))
        .route("/api/auth/magic-link",        post(magic_link_request))
        .route("/api/auth/magic-link/open",   get(magic_link_open))
        .route("/api/auth/magic-link/verify", post(magic_link_verify))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn apple(
    State(state):      State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers:           HeaderMap,
    AppJson(body):     AppJson<AppleAuthBody>,
) -> AppResult<Json<AuthResponse>> {
    let ip   = client_ip(&headers, Some(&ConnectInfo(addr)));
    let resp = service::authenticate_apple(
        &state.db, &state.cfg, &state.http,
        &body.identity_token, body.display_name.as_deref(),
        Some(ip), &state.event_bus,
    ).await?;
    Ok(Json(resp))
}

pub async fn me(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<MeResponse>> {
    let user = service::get_active_user(&state.db, user_id).await?;
    Ok(Json(MeResponse { user }))
}

pub async fn push_token(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<PushTokenBody>,
) -> AppResult<Json<serde_json::Value>> {
    service::update_push_token(&state.db, user_id, &body.push_token).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn display_name(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<DisplayNameBody>,
) -> AppResult<Json<serde_json::Value>> {
    let trimmed    = body.display_name.trim();
    let char_count = trimmed.chars().count();
    if char_count == 0 || char_count > 50 {
        return Err(AppError::bad_request("display_name must be 1–50 characters"));
    }
    service::update_display_name(&state.db, user_id, trimmed).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn logout(
    State(state): State<AppState>,
    RequireClaims(claims): RequireClaims,
) -> AppResult<Json<serde_json::Value>> {
    auth::revoke_token(&state.db, &state.redis, &state.revoked, claims.user_id, &claims.jti, claims.exp).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn magic_link_request(
    State(state):      State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers:           HeaderMap,
    AppJson(body):     AppJson<MagicLinkBody>,
) -> AppResult<Json<serde_json::Value>> {
    let ip = client_ip(&headers, Some(&ConnectInfo(addr)));
    service::request_magic_link(
        &state.db, &state.cfg, &state.http, state.redis.as_ref(), &body.email, Some(ip),
    ).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct MagicLinkOpenParams { token: String }

async fn magic_link_open(Query(params): Query<MagicLinkOpenParams>) -> Response {
    Redirect::temporary(&format!("whisked://auth?token={}", params.token)).into_response()
}

pub async fn magic_link_verify(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AppJson(body): AppJson<MagicLinkVerifyBody>,
) -> AppResult<Json<AuthResponse>> {
    let ip = client_ip(&headers, Some(&ConnectInfo(addr)));
    Ok(Json(
        service::verify_magic_link(
            &state.db, &state.cfg, state.redis.as_ref(), &body.token, Some(ip),
            &state.event_bus,
        ).await?
    ))
}
