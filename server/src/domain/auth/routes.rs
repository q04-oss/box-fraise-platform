use axum::{
    extract::State,
    routing::{get, patch, post},
    Json, Router,
};

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
        .route("/api/auth/demo",            post(demo))
        .route("/api/auth/register",        post(register))
        .route("/api/auth/login",           post(login))
        .route("/api/auth/me",              get(me))
        .route("/api/auth/push-token",      patch(push_token))
        .route("/api/auth/display-name",    patch(display_name))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route("/api/auth/reset-password",  post(reset_password))
        .route("/api/auth/claim-booking",   post(claim_booking))
        .route("/api/auth/logout",          post(logout))
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
    auth::revoke(&state.revoked, &claims.jti, claims.exp);
    Ok(Json(serde_json::json!({ "ok": true })))
}
