use axum::{
    extract::{ConnectInfo, Query, State},
    response::{Html, IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use std::net::SocketAddr;

use crate::{
    app::AppState,
    auth,
    error::{AppError, AppResult},
    http::extractors::{
        auth::{RequireClaims, RequireUser},
        json::AppJson,
    },
};
use super::{repository, service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/apple",           post(apple))
        .route("/api/auth/operator",        post(operator))
        .route("/api/auth/staff",           post(staff_login))
        .route("/api/auth/demo",            post(demo))
        .route("/api/auth/register",        post(register))
        .route("/api/auth/login",           post(login))
        .route("/api/auth/me",              get(me))
        .route("/api/auth/push-token",      patch(push_token))
        .route("/api/auth/display-name",    patch(display_name))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route("/api/auth/reset-password",  post(reset_password))
        .route("/api/auth/claim-booking",        post(claim_booking))
        .route("/api/auth/logout",               post(logout))
        .route("/api/auth/verify-email",         get(verify_email))
        .route("/api/auth/resend-verification",  post(resend_verification))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn apple(
    State(state): State<AppState>,
    AppJson(body): AppJson<AppleAuthBody>,
) -> AppResult<Json<AuthResponse>> {
    let resp = service::apple_sign_in(
        &state,
        &body.identity_token,
        body.display_name.as_deref(),
    )
    .await?;
    Ok(Json(resp))
}

async fn operator(
    State(state): State<AppState>,
    AppJson(body): AppJson<OperatorAuthBody>,
) -> AppResult<Json<AuthResponse>> {
    Ok(Json(service::operator_login(&state, &body.code, body.location_id).await?))
}

async fn staff_login(
    State(state):               State<AppState>,
    ConnectInfo(addr):          ConnectInfo<SocketAddr>,
    AppJson(body):              AppJson<StaffLoginBody>,
) -> AppResult<Json<StaffAuthResponse>> {
    Ok(Json(service::staff_login(&state, &body.pin, body.location_id, Some(addr.ip())).await?))
}

async fn demo(
    State(state): State<AppState>,
    AppJson(body): AppJson<DemoAuthBody>,
) -> AppResult<Json<AuthResponse>> {
    Ok(Json(service::demo_login(&state, &body.pin).await?))
}

async fn register(
    State(state): State<AppState>,
    AppJson(body): AppJson<RegisterBody>,
) -> AppResult<Json<AuthResponse>> {
    Ok(Json(
        service::register(&state, &body.email, &body.password, body.display_name.as_deref())
            .await?,
    ))
}

async fn login(
    State(state): State<AppState>,
    AppJson(body): AppJson<LoginBody>,
) -> AppResult<Json<AuthResponse>> {
    Ok(Json(service::login(&state, &body.email, &body.password).await?))
}

async fn me(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<MeResponse>> {
    let user = service::require_active(&state, user_id).await?;
    Ok(Json(MeResponse {
        user,
        table_bookings: vec![], // populated once the `table` domain is ported
    }))
}

async fn push_token(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<PushTokenBody>,
) -> AppResult<Json<serde_json::Value>> {
    repository::set_push_token(&state.db, user_id, &body.push_token).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn display_name(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<DisplayNameBody>,
) -> AppResult<Json<serde_json::Value>> {
    // Use char count, not byte length, so multi-byte characters aren't penalised.
    let trimmed = body.display_name.trim();
    let char_count = trimmed.chars().count();
    if char_count == 0 || char_count > 50 {
        return Err(AppError::bad_request("display_name must be 1–50 characters"));
    }
    repository::set_display_name(&state.db, user_id, trimmed).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn forgot_password(
    State(state): State<AppState>,
    AppJson(body): AppJson<ForgotPasswordBody>,
) -> AppResult<Json<serde_json::Value>> {
    // Always 200 to avoid email enumeration.
    service::forgot_password(&state, &body.email).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn reset_password(
    State(state): State<AppState>,
    AppJson(body): AppJson<ResetPasswordBody>,
) -> AppResult<Json<serde_json::Value>> {
    service::reset_password(&state, &body.token, &body.password).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn claim_booking(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<ClaimBookingBody>,
) -> AppResult<Json<serde_json::Value>> {
    let verified = repository::claim_booking_email(&state.db, user_id, &body.email).await?;
    Ok(Json(serde_json::json!({ "ok": true, "verified": verified })))
}

async fn logout(
    State(state): State<AppState>,
    RequireClaims(claims): RequireClaims,
) -> AppResult<Json<serde_json::Value>> {
    auth::revoke_token(&state.redis, &state.revoked, &claims.jti, claims.exp).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct VerifyEmailParams { token: String }

async fn verify_email(
    State(state):  State<AppState>,
    Query(params): Query<VerifyEmailParams>,
) -> Response {
    match service::verify_email(&state, &params.token).await {
        Ok(email) => Html(verify_page(true,  &email,  "")).into_response(),
        Err(_)    => Html(verify_page(false, "",      "this link has expired or already been used")).into_response(),
    }
}

async fn resend_verification(
    State(state):      State<AppState>,
    RequireUser(uid):  RequireUser,
) -> AppResult<Json<serde_json::Value>> {
    let user = repository::find_by_id(&state.db, uid)
        .await?
        .ok_or(AppError::NotFound)?;

    if user.verified {
        return Ok(Json(serde_json::json!({ "ok": true, "already_verified": true })));
    }

    service::resend_verification(&state, uid, &user.email).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn verify_page(ok: bool, email: &str, error: &str) -> String {
    let (icon, heading, detail) = if ok {
        ("✓", "email verified", format!("<p>{email}</p><p>you can now earn loyalty steeps.</p>"))
    } else {
        ("✗", "verification failed", format!("<p>{error}</p><p>open the app to request a new link.</p>"))
    };
    format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>box fraise · {heading}</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
     background:#F7F5F2;display:flex;align-items:center;justify-content:center;
     min-height:100vh}}
.card{{background:#fff;border-radius:20px;padding:48px 32px;max-width:360px;
      width:90%;text-align:center;box-shadow:0 4px 24px rgba(0,0,0,.07)}}
.icon{{font-size:3rem;margin-bottom:12px;color:{icon_color}}}
h1{{font-size:1.3rem;font-weight:600;color:#1C1C1E;margin-bottom:16px}}
p{{font-size:.875rem;color:#8E8E93;line-height:1.5;margin-bottom:8px}}
</style>
</head>
<body>
<div class="card">
  <div class="icon">{icon}</div>
  <h1>{heading}</h1>
  {detail}
</div>
</body>
</html>"#,
    icon_color = if ok { "#4CAF50" } else { "#C0392B" })
}
