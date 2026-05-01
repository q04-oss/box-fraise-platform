use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
};
use super::types::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/popups",                           get(list))
        .route("/api/popups/{id}",                       get(find))
        .route("/api/popups/{id}/rsvp",                  post(rsvp).delete(cancel_rsvp))
        .route("/api/popups/{id}/rsvp-status",           get(rsvp_status))
}

// ── List / find ───────────────────────────────────────────────────────────────

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<PopupRow>>> {
    let rows: Vec<PopupRow> = sqlx::query_as(
        "SELECT id, name, address, description, capacity,
                entrance_fee_cents, active, created_at
         FROM businesses
         WHERE business_type = 'popup' AND active = true
         ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn find(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> AppResult<Json<PopupRow>> {
    sqlx::query_as::<_, PopupRow>(
        "SELECT id, name, address, description, capacity,
                entrance_fee_cents, active, created_at
         FROM businesses
         WHERE id = $1 AND business_type = 'popup'",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)
    .map(Json)
}

// ── RSVP ──────────────────────────────────────────────────────────────────────

async fn rsvp(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(popup_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    // Check capacity.
    let popup = sqlx::query_as::<_, PopupRow>(
        "SELECT id, name, address, description, capacity,
                entrance_fee_cents, active, created_at
         FROM businesses WHERE id = $1 AND business_type = 'popup'",
    )
    .bind(popup_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)?;

    // Count existing confirmed RSVPs.
    let confirmed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM popup_rsvps
         WHERE business_id = $1 AND status = 'confirmed'",
    )
    .bind(popup_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    let at_capacity = popup.capacity.map_or(false, |c| confirmed >= c as i64);

    if popup.entrance_fee_cents.unwrap_or(0) > 0 {
        // Paid event — create Stripe PI, RSVP pending payment.
        let pi = state
            .stripe()
            .create_payment_intent(
                popup.entrance_fee_cents.unwrap() as i64,
                "cad",
                None,
                &[
                    ("type", "rsvp"),
                    ("popup_id", &popup_id.to_string()),
                    ("user_id", &user_id.to_string()),
                ],
            )
            .await?;

        sqlx::query(
            "INSERT INTO popup_rsvps (user_id, business_id, status, stripe_payment_intent_id)
             VALUES ($1, $2, 'pending', $3)
             ON CONFLICT (user_id, business_id) DO UPDATE
             SET status = 'pending',
                 stripe_payment_intent_id = EXCLUDED.stripe_payment_intent_id",
        )
        .bind(user_id)
        .bind(popup_id)
        .bind(&pi.id)
        .execute(&state.db)
        .await
        .map_err(AppError::Db)?;

        Ok(Json(serde_json::json!({
            "status":        "pending_payment",
            "client_secret": pi.client_secret,
            "at_capacity":   at_capacity,
        })))
    } else {
        // Free event — confirm immediately.
        let status = if at_capacity { "waitlist" } else { "confirmed" };

        sqlx::query(
            "INSERT INTO popup_rsvps (user_id, business_id, status)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id, business_id) DO UPDATE SET status = EXCLUDED.status",
        )
        .bind(user_id)
        .bind(popup_id)
        .bind(status)
        .execute(&state.db)
        .await
        .map_err(AppError::Db)?;

        Ok(Json(serde_json::json!({ "status": status })))
    }
}

async fn cancel_rsvp(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(popup_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    sqlx::query(
        "DELETE FROM popup_rsvps
         WHERE user_id = $1 AND business_id = $2",
    )
    .bind(user_id)
    .bind(popup_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn rsvp_status(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(popup_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM popup_rsvps
         WHERE user_id = $1 AND business_id = $2",
    )
    .bind(user_id)
    .bind(popup_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({ "status": status })))
}

