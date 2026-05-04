use axum::{
    extract::{Path, State},
    routing::{get, patch},
    Json, Router,
};

use box_fraise_domain::domain::platform_configuration::{
    service,
    types::{
        PlatformConfigurationHistoryResponse, PlatformConfigurationResponse,
        UpdateConfigurationRequest,
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
        .route("/api/admin/configuration",                 get(list_all))
        .route("/api/admin/configuration/{key}",           get(get_one).patch(update_one))
        .route("/api/admin/configuration/{key}/history",   get(get_history))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/admin/configuration
///
/// Return all platform configuration values. Requires platform_admin.
async fn list_all(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<PlatformConfigurationResponse>>> {
    Ok(Json(service::get_all_configuration(&state.db, user_id).await?))
}

/// GET /api/admin/configuration/:key
///
/// Return a single configuration value. Requires platform_admin.
async fn get_one(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(key):            Path<String>,
) -> AppResult<Json<PlatformConfigurationResponse>> {
    // Verify admin before returning.
    let user = box_fraise_domain::domain::auth::repository::find_by_id(
        &state.db, user_id,
    ).await?.ok_or(box_fraise_domain::error::DomainError::Unauthorized)?;
    if !user.is_platform_admin {
        return Err(box_fraise_domain::error::DomainError::Forbidden.into());
    }
    Ok(Json(service::get_configuration(&state.db, &key).await?))
}

/// PATCH /api/admin/configuration/:key
///
/// Update a configuration value. Requires platform_admin.
/// Returns 400 if value fails type validation.
async fn update_one(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(key):            Path<String>,
    AppJson(body):        AppJson<UpdateConfigurationRequest>,
) -> AppResult<Json<PlatformConfigurationResponse>> {
    Ok(Json(service::update_configuration(&state.db, &key, user_id, body).await?))
}

/// GET /api/admin/configuration/:key/history
///
/// Return full change history for a configuration key. Requires platform_admin.
async fn get_history(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(key):            Path<String>,
) -> AppResult<Json<Vec<PlatformConfigurationHistoryResponse>>> {
    Ok(Json(service::get_configuration_history(&state.db, &key, user_id).await?))
}
