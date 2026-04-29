use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::types::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/gifts",               get(list).post(send))
        .route("/api/gifts/claim/:token",  post(claim))
}

async fn list(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<GiftRow>>> {
    let rows: Vec<GiftRow> = sqlx::query_as(
        "SELECT id, sender_id, recipient_email, recipient_phone,
                gift_type, amount_cents, claim_token, claimed_at, created_at
         FROM gifts
         WHERE sender_id = $1
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn send(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<SendGiftBody>,
) -> AppResult<Json<GiftRow>> {
    if body.recipient_email.is_none() && body.recipient_phone.is_none() {
        return Err(AppError::bad_request("recipient_email or recipient_phone is required"));
    }

    let valid_types = ["digital", "physical", "bundle"];
    if !valid_types.contains(&body.gift_type.as_str()) {
        return Err(AppError::bad_request("invalid gift_type"));
    }

    let claim_token = Uuid::new_v4().to_string();

    let row: GiftRow = sqlx::query_as(
        "INSERT INTO gifts
             (sender_id, recipient_email, recipient_phone, gift_type, amount_cents, claim_token)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, sender_id, recipient_email, recipient_phone,
                   gift_type, amount_cents, claim_token, claimed_at, created_at",
    )
    .bind(user_id)
    .bind(body.recipient_email.as_deref())
    .bind(body.recipient_phone.as_deref())
    .bind(&body.gift_type)
    .bind(body.amount_cents)
    .bind(&claim_token)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    // TODO: send gift notification email via integrations::resend

    Ok(Json(row))
}

/// Claim a gift by its single-use token. Atomically marks it claimed.
async fn claim(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(token): Path<String>,
) -> AppResult<Json<GiftRow>> {
    let row: Option<GiftRow> = sqlx::query_as(
        "UPDATE gifts
         SET claimed_at = NOW(), claimed_by_user_id = $2
         WHERE claim_token = $1
           AND claimed_at IS NULL
         RETURNING id, sender_id, recipient_email, recipient_phone,
                   gift_type, amount_cents, claim_token, claimed_at, created_at",
    )
    .bind(&token)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;

    row.ok_or_else(|| AppError::bad_request("invalid or already claimed gift token"))
        .map(Json)
}
