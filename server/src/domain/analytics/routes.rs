//! Analytics routes — every endpoint is `platform_admin`-only.
//!
//! There is no global "admin middleware" in this codebase; the established
//! pattern is `RequireUser` + a service-layer `is_platform_admin` check.
//! `require_platform_admin` below is the single check helper for this module.

use axum::{extract::State, routing::get, Json, Router};
use box_fraise_domain::types::UserId;
use sqlx::PgPool;

use super::queries::{
    background_check_stats, business_stats, conversion_dropoff, daily_attestations,
    daily_presence_events, soultoken_stats, time_to_attest, verification_funnel,
    BackgroundCheckStats, BusinessStats, DailyCount, DropOff, FunnelStage, SoultokenStats,
    TimeToAttest,
};
use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/analytics/funnel",                       get(funnel))
        .route("/api/admin/analytics/attestations/daily",           get(attestations_daily))
        .route("/api/admin/analytics/attestations/time-to-attest",  get(attestations_time_to_attest))
        .route("/api/admin/analytics/businesses",                   get(businesses))
        .route("/api/admin/analytics/presence/daily",               get(presence_daily))
        .route("/api/admin/analytics/soultokens",                   get(soultokens))
        .route("/api/admin/analytics/background-checks",            get(background_checks))
        .route("/api/admin/analytics/conversion",                   get(conversion))
}

// ── Auth helper ───────────────────────────────────────────────────────────────

async fn require_platform_admin(pool: &PgPool, user_id: UserId) -> Result<(), AppError> {
    let is_admin: Option<bool> = sqlx::query_scalar(
        "SELECT is_platform_admin FROM users WHERE id = $1 AND deleted_at IS NULL"
    )
    .bind(i32::from(user_id))
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    match is_admin {
        Some(true)  => Ok(()),
        _           => Err(AppError::Forbidden),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

#[utoipa::path(
    get, path = "/api/admin/analytics/funnel", tag = "analytics",
    responses((status = 200, description = "Verification funnel snapshot")),
    security(("bearer_auth" = [])),
)]
pub async fn funnel(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<FunnelStage>>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(verification_funnel(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/attestations/daily", tag = "analytics",
    responses((status = 200, description = "Soultokens issued per day (last 30 days)")),
    security(("bearer_auth" = [])),
)]
pub async fn attestations_daily(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<DailyCount>>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(daily_attestations(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/attestations/time-to-attest", tag = "analytics",
    responses((status = 200, description = "Days from identity_confirmed to soultoken_issued")),
    security(("bearer_auth" = [])),
)]
pub async fn attestations_time_to_attest(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<TimeToAttest>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(time_to_attest(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/businesses", tag = "analytics",
    responses((status = 200, description = "Business activation stats")),
    security(("bearer_auth" = [])),
)]
pub async fn businesses(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<BusinessStats>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(business_stats(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/presence/daily", tag = "analytics",
    responses((status = 200, description = "Presence events per day (last 14 days)")),
    security(("bearer_auth" = [])),
)]
pub async fn presence_daily(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<DailyCount>>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(daily_presence_events(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/soultokens", tag = "analytics",
    responses((status = 200, description = "Soultoken issuance and renewal stats")),
    security(("bearer_auth" = [])),
)]
pub async fn soultokens(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<SoultokenStats>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(soultoken_stats(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/background-checks", tag = "analytics",
    responses((status = 200, description = "Background-check pass rate by check type")),
    security(("bearer_auth" = [])),
)]
pub async fn background_checks(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<BackgroundCheckStats>>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(background_check_stats(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/admin/analytics/conversion", tag = "analytics",
    responses((status = 200, description = "Adjacent-stage conversion rates")),
    security(("bearer_auth" = [])),
)]
pub async fn conversion(
    State(state):         State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<DropOff>>> {
    require_platform_admin(&state.db, user_id).await?;
    Ok(Json(conversion_dropoff(&state.db).await?))
}
