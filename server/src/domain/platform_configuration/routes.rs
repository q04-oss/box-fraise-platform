use axum::{
    extract::{Path, State},
    routing::{get, patch},
    Json, Router,
};

use box_fraise_domain::domain::platform_configuration::{
    service,
    types::{
        FeatureFlagRow, PlatformConfigurationHistoryResponse, PlatformConfigurationResponse,
        UpdateConfigurationRequest, UpdateFeatureFlagRequest,
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
        // Hardening §10 — feature flags admin surface.
        .route("/api/admin/feature-flags",                 get(list_flags))
        .route("/api/admin/feature-flags/{flag_name}",     patch(update_flag))
        // Hardening §10 — billing read surface.
        .route("/api/admin/billing/subscriptions",         get(list_subscriptions))
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

/// GET /api/admin/feature-flags — list every flag (Hardening §10).
async fn list_flags(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<FeatureFlagRow>>> {
    Ok(Json(service::list_feature_flags(&state.db, user_id).await?))
}

/// GET /api/admin/billing/subscriptions — Hardening §10 read-only surface.
/// Sized for the admin dashboard to inspect existing rows; future write
/// paths will land alongside Stripe webhook integration.
async fn list_subscriptions(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<serde_json::Value>>> {
    let user = box_fraise_domain::domain::auth::repository::find_by_id(&state.db, user_id)
        .await?
        .ok_or(box_fraise_domain::error::DomainError::Unauthorized)?;
    if !user.is_platform_admin {
        return Err(box_fraise_domain::error::DomainError::Forbidden.into());
    }
    let rows: Vec<(i32, i32, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT id, business_id, tier, stripe_subscription_id, current_period_end \
             FROM business_subscriptions ORDER BY id"
        )
        .fetch_all(&state.db)
        .await
        .map_err(box_fraise_domain::error::DomainError::Db)?;
    let out: Vec<serde_json::Value> = rows.into_iter().map(|(id, biz, tier, sub, end)| {
        serde_json::json!({
            "id": id, "business_id": biz, "tier": tier,
            "stripe_subscription_id": sub, "current_period_end": end,
        })
    }).collect();
    Ok(Json(out))
}

/// PATCH /api/admin/feature-flags/:flag_name — flip global enabled.
async fn update_flag(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(flag_name):      Path<String>,
    AppJson(body):        AppJson<UpdateFeatureFlagRequest>,
) -> AppResult<Json<serde_json::Value>> {
    service::set_feature_flag_enabled(&state.db, user_id, &flag_name, body.enabled).await?;
    Ok(Json(serde_json::json!({ "flag_name": flag_name, "enabled": body.enabled })))
}
