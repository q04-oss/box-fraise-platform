//! Analytics routes — every endpoint is `platform_admin`-only.
//!
//! There is no global "admin middleware" in this codebase; the established
//! pattern is `RequireUser` + a service-layer `is_platform_admin` check.
//! `require_platform_admin` below is the single check helper for this module.
//!
//! Reads run inside an `AdminRlsTransaction` so that under `app_user` (RLS
//! active) the queries see all rows via the admin-bypass policies. Under
//! `fraise` (BYPASSRLS) the tag is harmless.

use axum::{extract::State, routing::get, Json, Router};
use box_fraise_domain::{
    domain::auth::repository as user_repo,
    transaction::AdminRlsTransaction,
    types::UserId,
};
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
    let user = user_repo::find_by_id(pool, user_id)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::Forbidden)?;
    if user.is_platform_admin { Ok(()) } else { Err(AppError::Forbidden) }
}

async fn admin_tx(pool: &PgPool) -> AppResult<AdminRlsTransaction> {
    AdminRlsTransaction::begin(pool).await.map_err(AppError::from)
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
    let mut tx = admin_tx(&state.db).await?;
    let out = verification_funnel(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = daily_attestations(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = time_to_attest(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = business_stats(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = daily_presence_events(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = soultoken_stats(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = background_check_stats(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
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
    let mut tx = admin_tx(&state.db).await?;
    let out = conversion_dropoff(tx.as_mut()).await.map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(out))
}
