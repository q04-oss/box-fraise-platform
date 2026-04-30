/// Axum request extractors for authenticated callers.
///
/// `RequireUser`   — any valid, non-revoked user JWT. Yields the user's ID.
/// `RequireClaims` — same, but yields the full Claims (needed for logout).
/// `OptionalAuth`  — yields Some(user_id) if a valid token is present, None otherwise.
/// `RequireStaff`  — valid StaffClaims JWT signed with STAFF_JWT_SECRET. Yields
///                   (user_id, business_id). A regular user JWT is rejected at the
///                   cryptographic level before any claim check runs.
/// `RequireDevice` — Cardputer EIP-191 device auth. Yields DeviceInfo.
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use sqlx::FromRow;

use crate::{
    app::AppState,
    auth::device::{parse_auth_header, verify_signature},
    auth::staff::{self as staff_auth, StaffClaims},
    auth::{self, Claims},
    error::AppError,
    types::UserId,
};

// ── RequireClaims ─────────────────────────────────────────────────────────────

pub struct RequireClaims(pub Claims);

impl<S> FromRequestParts<S> for RequireClaims
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let app = AppState::from_ref(state);

        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let claims = auth::verify_token(bearer.token(), &app.cfg).ok_or(AppError::Unauthorized)?;

        if auth::check_revoked(&app.redis, &app.revoked, &claims.jti).await {
            return Err(AppError::Unauthorized);
        }

        Ok(RequireClaims(claims))
    }
}

// ── RequireUser ───────────────────────────────────────────────────────────────

pub struct RequireUser(pub UserId);

impl<S> FromRequestParts<S> for RequireUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let RequireClaims(claims) = RequireClaims::from_request_parts(parts, state).await?;
        Ok(RequireUser(claims.user_id))
    }
}

// ── OptionalAuth ──────────────────────────────────────────────────────────────

pub struct OptionalAuth(pub Option<UserId>);

impl<S> FromRequestParts<S> for OptionalAuth
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        match RequireUser::from_request_parts(parts, state).await {
            Ok(RequireUser(id)) => Ok(OptionalAuth(Some(id))),
            Err(_) => Ok(OptionalAuth(None)),
        }
    }
}

// ── RequireDevice ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: i32,
    pub address: String,
    pub role: String,
    pub user_id: Option<UserId>,
    pub business_id: Option<i32>,
}

pub struct RequireDevice(pub DeviceInfo);

impl<S> FromRequestParts<S> for RequireDevice
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let app = AppState::from_ref(state);

        let auth_value = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let header = parse_auth_header(auth_value).ok_or(AppError::Unauthorized)?;

        let recovered = verify_signature(&header)?;

        #[derive(FromRow)]
        struct Row {
            id: i32,
            role: String,
            user_id: Option<UserId>,
            business_id: Option<i32>,
        }

        let row: Option<Row> = sqlx::query_as(
            "SELECT id, role, user_id, business_id \
             FROM devices \
             WHERE LOWER(device_address) = LOWER($1) \
             LIMIT 1",
        )
        .bind(&recovered)
        .fetch_optional(&app.db)
        .await
        .map_err(AppError::Db)?;

        let row = row.ok_or(AppError::Unauthorized)?;

        Ok(RequireDevice(DeviceInfo {
            id: row.id,
            address: recovered,
            role: row.role,
            user_id: row.user_id,
            business_id: row.business_id,
        }))
    }
}

// ── RequireStaff ──────────────────────────────────────────────────────────────

/// Verified staff credentials scoped to a specific business.
///
/// A regular user JWT is rejected at the signature-verification step —
/// it was not signed with STAFF_JWT_SECRET. A staff token for business A
/// cannot satisfy an endpoint that asserts business B's ID.
pub struct RequireStaff(pub StaffClaims);

impl RequireStaff {
    pub fn user_id(&self) -> UserId {
        self.0.user_id
    }
    pub fn business_id(&self) -> i32 {
        self.0.business_id
    }
}

impl<S> FromRequestParts<S> for RequireStaff
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let app = AppState::from_ref(state);

        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let claims = staff_auth::verify_staff_token(bearer.token(), &app.cfg)
            .ok_or(AppError::Unauthorized)?;

        // Staff tokens are revocation-checked via Redis. Logout, admin-forced
        // termination, and staff removal all trigger revocation. The 8-hour TTL
        // is defense-in-depth alongside revocation, not a substitute for it.
        if auth::check_revoked(&app.redis, &app.revoked, &claims.jti).await {
            return Err(AppError::Unauthorized);
        }

        Ok(RequireStaff(claims))
    }
}
