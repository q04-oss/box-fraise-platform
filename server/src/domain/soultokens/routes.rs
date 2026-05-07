use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use secrecy::ExposeSecret;
use serde_json::json;

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
        // Public — no auth — soultoken trust registry (Hardening Section 1b).
        .route("/api/trust-registry/public-key", get(trust_registry_public_key))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/soultokens/issue
///
/// Issue a soultoken after attestation is approved.
/// Requesting user must have `verification_status = 'attested'`.
#[utoipa::path(
    post, path = "/api/soultokens/issue", tag = "soultokens",
    request_body = IssueSoultokenRequest,
    responses((status = 201, description = "Soultoken issued")),
    security(("bearer_auth" = [])),
)]
pub async fn issue(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<IssueSoultokenRequest>,
) -> AppResult<(StatusCode, Json<SoultokenResponse>)> {
    let hmac_key = state.cfg.soultoken_hmac_key.expose_secret().as_bytes().to_vec();
    let resp = service::issue_soultoken(
        &state.db, user_id, body, &hmac_key, &state.ed25519_key_pair, &state.event_bus,
    ).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/soultokens/me
///
/// Return the active soultoken for the authenticated user.
/// Response contains `display_code`, never `uuid`.
#[utoipa::path(
    get, path = "/api/soultokens/me", tag = "soultokens",
    responses((status = 200, description = "Active soultoken for the authenticated user")),
    security(("bearer_auth" = [])),
)]
pub async fn get_mine(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<SoultokenResponse>> {
    Ok(Json(service::get_my_soultoken(&state.db, user_id).await?))
}

/// POST /api/soultokens/renew
///
/// Renew the authenticated user's active soultoken for 12 more months.
#[utoipa::path(
    post, path = "/api/soultokens/renew", tag = "soultokens",
    request_body = RenewSoultokenRequest,
    responses((status = 200, description = "Soultoken renewed")),
    security(("bearer_auth" = [])),
)]
pub async fn renew(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<RenewSoultokenRequest>,
) -> AppResult<Json<SoultokenRenewalResponse>> {
    Ok(Json(
        service::renew_soultoken(
            &state.db, user_id, body, &state.ed25519_key_pair, &state.event_bus,
        ).await?,
    ))
}

/// GET /api/trust-registry/public-key
///
/// Public endpoint — no auth required. Publishes the Ed25519 verifying key
/// any third party can use to verify Box Fraise soultoken signatures offline.
#[utoipa::path(
    get, path = "/api/trust-registry/public-key", tag = "soultokens",
    responses((status = 200, description = "Ed25519 verifying key for offline soultoken signature verification")),
)]
pub async fn trust_registry_public_key(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(json!({
        "verifying_key_hex": state.cfg.soultoken_verifying_key_hex,
        "algorithm":         "Ed25519",
        "bfip_version":      "0.2.0",
        "description":       "Use this key to verify Box Fraise \
                              soultoken signatures offline",
    }))
}

/// POST /api/soultokens/:id/revoke
///
/// Revoke a soultoken. Requires platform_admin or attestation_reviewer role.
#[utoipa::path(
    post, path = "/api/soultokens/{id}/revoke", tag = "soultokens",
    params(("id" = i32, Path, description = "Soultoken ID to revoke")),
    request_body = RevokeSoultokenRequest,
    responses((status = 200, description = "Soultoken revoked")),
    security(("bearer_auth" = [])),
)]
pub async fn revoke(
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
#[utoipa::path(
    post, path = "/api/soultokens/{id}/surrender", tag = "soultokens",
    params(("id" = i32, Path, description = "Soultoken ID to surrender")),
    request_body = SurrenderSoultokenRequest,
    responses((status = 200, description = "Soultoken surrendered")),
    security(("bearer_auth" = [])),
)]
pub async fn surrender(
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
