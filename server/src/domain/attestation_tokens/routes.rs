use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};

use box_fraise_domain::domain::attestation_tokens::{
    service,
    types::{
        AttestationTokenMeta, AttestationTokenResponse, IssueAttestationTokenRequest,
        VerificationResultResponse, VerifyAttestationTokenRequest,
    },
};
use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::{auth::RequireUser, json::AppJson},
};

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/attestation-tokens/issue",    post(issue))
        .route("/api/attestation-tokens/verify",   post(verify))
        .route("/api/attestation-tokens/me",       get(get_my_tokens))
        .route("/api/attestation-tokens/{id}/revoke", post(revoke))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/attestation-tokens/issue
///
/// Issue a short-lived scoped attestation token.
/// Returns 201 with the raw_token — this is the ONLY time it is returned.
/// Returns 404 if the user has no active soultoken.
#[utoipa::path(
    post, path = "/api/attestation-tokens/issue", tag = "attestation-tokens",
    request_body = IssueAttestationTokenRequest,
    responses(
        (status = 201, description = "Token issued; raw_token returned ONCE", body = AttestationTokenResponse),
        (status = 404, description = "User has no active soultoken"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn issue(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body):        AppJson<IssueAttestationTokenRequest>,
) -> AppResult<(StatusCode, Json<AttestationTokenResponse>)> {
    let resp = service::issue_token(&state.db, user_id, body, &state.event_bus).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// POST /api/attestation-tokens/verify
///
/// Verify a presented raw token. No auth required — third parties call this.
/// Always returns 200; the `valid` and `outcome` fields signal the result.
/// Never reveals token existence via status code.
#[utoipa::path(
    post, path = "/api/attestation-tokens/verify", tag = "attestation-tokens",
    request_body = VerifyAttestationTokenRequest,
    responses(
        (status = 200, description = "Verification result; check `valid` and `outcome` fields", body = VerificationResultResponse),
    ),
)]
pub async fn verify(
    State(state): State<AppState>,
    headers:      HeaderMap,
    AppJson(body): AppJson<VerifyAttestationTokenRequest>,
) -> AppResult<Json<VerificationResultResponse>> {
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let result = service::verify_token(
        &state.db,
        body,
        None,
        user_agent,
        &state.event_bus,
    ).await?;
    Ok(Json(result))
}

/// GET /api/attestation-tokens/me
///
/// Return the authenticated user's issued tokens.
/// raw_token is never included in list responses.
#[utoipa::path(
    get, path = "/api/attestation-tokens/me", tag = "attestation-tokens",
    responses(
        (status = 200, description = "List of caller's attestation tokens (metadata only)", body = [AttestationTokenMeta]),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn get_my_tokens(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<AttestationTokenMeta>>> {
    Ok(Json(service::get_my_tokens(&state.db, user_id).await?))
}

/// POST /api/attestation-tokens/:id/revoke
///
/// Revoke an attestation token before it expires.
/// Returns 403 if the caller does not own the token.
#[utoipa::path(
    post, path = "/api/attestation-tokens/{id}/revoke", tag = "attestation-tokens",
    params(("id" = i32, Path, description = "Attestation token id to revoke")),
    responses(
        (status = 200, description = "Token revoked"),
        (status = 403, description = "Caller does not own the token"),
    ),
    security(("bearer_auth" = [])),
)]
pub async fn revoke(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(token_id):       Path<i32>,
) -> AppResult<StatusCode> {
    service::revoke_my_token(&state.db, token_id, user_id).await?;
    Ok(StatusCode::OK)
}
