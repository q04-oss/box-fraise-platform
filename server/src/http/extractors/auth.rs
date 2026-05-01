//! Axum request extractors for authenticated callers.
//!
//! `RequireUser`   — any valid, non-revoked user JWT. Yields the user's ID.
//! `RequireClaims` — same, but yields the full Claims (needed for logout).
//! `OptionalAuth`  — yields Some(user_id) if a valid token is present, None otherwise.
//! `RequireStaff`  — valid StaffClaims JWT signed with STAFF_JWT_SECRET. Yields
//!                   (user_id, business_id). A regular user JWT is rejected at the
//!                   cryptographic level before any claim check runs.
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};

use crate::{
    app::AppState,
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

// ── RequireStaff ──────────────────────────────────────────────────────────────

/// Verified staff credentials scoped to a specific business.
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

        if auth::check_revoked(&app.redis, &app.revoked, &claims.jti).await {
            return Err(AppError::Unauthorized);
        }

        Ok(RequireStaff(claims))
    }
}
