use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use box_fraise_domain::domain::verification_events::{
    service,
    types::{UserAuditTrailResponse, VerificationEventResponse},
};
use crate::{
    app::AppState,
    error::AppResult,
    http::extractors::auth::RequireUser,
};

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/audit/trail",           get(my_trail))
        .route("/api/audit/journey",          get(my_journey))
        .route("/api/admin/audit/{user_id}", get(admin_trail))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/audit/trail
///
/// Return the authenticated user's complete audit trail (BFIP Section 17.1).
/// Records the access request in `audit_request_log` for compliance.
async fn my_trail(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<UserAuditTrailResponse>> {
    Ok(Json(service::get_my_audit_trail(&state.db, user_id).await?))
}

/// GET /api/audit/journey
///
/// Return the authenticated user's verification journey events only.
/// Lighter than the full audit trail — suitable for in-app status display.
async fn my_journey(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<VerificationEventResponse>>> {
    Ok(Json(service::get_verification_journey(&state.db, user_id).await?))
}

/// GET /api/admin/audit/:user_id
///
/// Return any user's full audit trail. Requires platform_admin.
/// Returns 403 if the caller is not a platform admin.
async fn admin_trail(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(target_id):      Path<i32>,
) -> AppResult<Json<UserAuditTrailResponse>> {
    Ok(Json(service::get_admin_audit_trail(&state.db, user_id, target_id).await?))
}
