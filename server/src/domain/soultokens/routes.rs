use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use secrecy::ExposeSecret;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::{service, types::*};

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/soultokens/issue",        post(issue))
        .route("/api/soultokens/me",           get(get_mine))
        .route("/api/soultokens/renew",        post(renew))
        .route("/api/soultokens/{id}/revoke",  post(revoke))
        .route("/api/soultokens/{id}/surrender", post(surrender))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/soultokens/issue
///
/// Issue a soultoken after attestation is approved.
/// Requesting user must have `verification_status = 'attested'`.
async fn issue(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<IssueSoultokenRequest>,
) -> AppResult<(StatusCode, Json<SoultokenResponse>)> {
    let hmac_key    = state.cfg.soultoken_hmac_key.expose_secret().as_bytes().to_vec();
    let signing_key = state.cfg.soultoken_signing_key.expose_secret().as_bytes().to_vec();
    let resp = service::issue_soultoken(
        &state.db, user_id, body, &hmac_key, &signing_key, &state.event_bus,
    ).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/soultokens/me
///
/// Return the active soultoken for the authenticated user.
/// Response contains `display_code`, never `uuid`.
async fn get_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<SoultokenResponse>> {
    Ok(Json(service::get_my_soultoken(&state.db, user_id).await?))
}

/// POST /api/soultokens/renew
///
/// Renew the authenticated user's active soultoken for 12 more months.
async fn renew(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<RenewSoultokenRequest>,
) -> AppResult<Json<SoultokenRenewalResponse>> {
    Ok(Json(
        service::renew_soultoken(&state.db, user_id, body, &state.event_bus).await?,
    ))
}

/// POST /api/soultokens/:id/revoke
///
/// Revoke a soultoken. Requires platform_admin or attestation_reviewer role.
async fn revoke(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(soultoken_id):   Path<i32>,
    AppJson(body):        AppJson<RevokeSoultokenRequest>,
) -> AppResult<Json<SoultokenResponse>> {
    Ok(Json(
        service::revoke_soultoken(&state.db, soultoken_id, user_id, body).await?,
    ))
}

/// POST /api/soultokens/:id/surrender
///
/// Voluntary surrender by the soultoken holder. Requires in-person visit and staff witness.
async fn surrender(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(soultoken_id):   Path<i32>,
    AppJson(body):        AppJson<SurrenderSoultokenRequest>,
) -> AppResult<Json<SoultokenResponse>> {
    Ok(Json(
        service::surrender_soultoken(
            &state.db, soultoken_id, user_id, body, &state.event_bus,
        ).await?,
    ))
}
