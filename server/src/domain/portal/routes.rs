/// Portal — creator monetization and identity verification.
///
/// Security model:
///   - Content is gated behind portal_access rows; a user can only read
///     content if they have a non-expired access row for the creator.
///   - Identity verification is initiated by a shop operator (physical ID
///     scan) and confirmed by Stripe Identity webhook — the server never
///     trusts a client-side "I am verified" claim.
///   - Portal opt-in requires explicit consent (GDPR / privacy).
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/portal/me",                get(portal_me))
        .route("/api/portal/access/:user_id",   post(subscribe))
        .route("/api/portal/content/:user_id",  get(content))
        .route("/api/portal/verify/cost",       get(verify_cost))
}

// ── Portal status ─────────────────────────────────────────────────────────────

async fn portal_me(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        portal_opted_in:  bool,
        identity_verified: bool,
        identity_verified_expires_at: Option<NaiveDateTime>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT portal_opted_in, identity_verified, identity_verified_expires_at
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;

    let Some(row) = row else { return Err(AppError::NotFound); };

    // Count active subscribers.
    let subscriber_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM portal_access
         WHERE owner_id = $1
           AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "portal_opted_in":              row.portal_opted_in,
        "identity_verified":            row.identity_verified,
        "identity_verified_expires_at": row.identity_verified_expires_at,
        "subscriber_count":             subscriber_count,
    })))
}

// ── Subscription ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SubscribeBody {
    /// Access duration in days. Defaults to 30.
    days: Option<i64>,
}

async fn subscribe(
    State(state): State<AppState>,
    RequireUser(buyer_id): RequireUser,
    Path(owner_id): Path<i32>,
    axum::extract::Json(body): axum::extract::Json<SubscribeBody>,
) -> AppResult<Json<serde_json::Value>> {
    if buyer_id == owner_id {
        return Err(AppError::bad_request("cannot subscribe to your own portal"));
    }

    // Verify the owner is opted in to the portal.
    let opted_in: bool = sqlx::query_scalar(
        "SELECT COALESCE(portal_opted_in, false) FROM users WHERE id = $1",
    )
    .bind(owner_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .unwrap_or(false);

    if !opted_in {
        return Err(AppError::bad_request("this creator has not enabled portal access"));
    }

    let days = body.days.unwrap_or(30).max(1).min(365);
    let expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(days))
        .unwrap()
        .naive_utc();

    // Create Stripe PI for the access fee.
    // The portal cut is applied at the webhook level when capturing.
    let access_fee_cents = 1000_i64; // CA$10 default; TODO: make per-user configurable

    let pi = state
        .stripe()
        .create_payment_intent(
            access_fee_cents,
            "cad",
            None,
            &[
                ("type", "portal_access"),
                ("owner_id", &owner_id.to_string()),
                ("buyer_id", &buyer_id.to_string()),
            ],
        )
        .await?;

    // Pre-create the access row (pending payment).
    sqlx::query(
        "INSERT INTO portal_access (buyer_id, owner_id, expires_at, status)
         VALUES ($1, $2, $3, 'pending')
         ON CONFLICT (buyer_id, owner_id) DO UPDATE
         SET expires_at = EXCLUDED.expires_at, status = 'pending'",
    )
    .bind(buyer_id)
    .bind(owner_id)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "client_secret":     pi.client_secret,
        "access_fee_cents":  access_fee_cents,
        "expires_at":        expires_at,
    })))
}

// ── Content access ────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Serialize)]
struct ContentRow {
    id:        i32,
    user_id:   i32,
    media_url: Option<String>,
    caption:   Option<String>,
    content_type: Option<String>,
    created_at: NaiveDateTime,
}

/// Serve a creator's portal content — requires active subscription.
async fn content(
    State(state): State<AppState>,
    RequireUser(buyer_id): RequireUser,
    Path(owner_id): Path<i32>,
) -> AppResult<Json<Vec<ContentRow>>> {
    // The creator can always view their own content.
    let is_owner = buyer_id == owner_id;

    if !is_owner {
        // Verify active, non-expired subscription.
        let has_access: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM portal_access
                 WHERE buyer_id = $1 AND owner_id = $2
                   AND status = 'active'
                   AND (expires_at IS NULL OR expires_at > NOW())
             )",
        )
        .bind(buyer_id)
        .bind(owner_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Db)?;

        if !has_access {
            return Err(AppError::Forbidden);
        }
    }

    let rows: Vec<ContentRow> = sqlx::query_as(
        "SELECT id, user_id, media_url, caption, content_type, created_at
         FROM portal_content
         WHERE user_id = $1
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(owner_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

// ── Verification cost ─────────────────────────────────────────────────────────

async fn verify_cost() -> Json<serde_json::Value> {
    // CA$111 initial verification, CA$333 annual renewal.
    Json(serde_json::json!({
        "initial_cents": 11_100,
        "renewal_cents": 33_300,
    }))
}
