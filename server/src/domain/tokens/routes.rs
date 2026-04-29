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

use super::types::{
    ContentTokenRow, EveningTokenRow, MintPortraitBody, PortraitTokenRow, TradeOfferBody,
    TradeOfferRow,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Evening tokens
        .route("/api/tokens/evening",                   get(list_evening_tokens))
        .route("/api/tokens/evening/confirm/:booking_id", post(confirm_evening_token))
        // Content tokens
        .route("/api/tokens/content",                   get(list_content_tokens))
        .route("/api/tokens/content/trade",             post(create_trade_offer))
        .route("/api/tokens/content/trade/:offer_id/accept",  post(accept_trade))
        .route("/api/tokens/content/trade/:offer_id/decline", post(decline_trade))
        // Portrait tokens
        .route("/api/tokens/portrait",                  get(list_portrait_tokens))
        .route("/api/tokens/portrait/mint",             post(mint_portrait))
        .route("/api/tokens/portrait/:token_id/buy",    post(buy_portrait))
}

// ── Evening tokens ────────────────────────────────────────────────────────────

/// List all evening tokens for the authenticated user.
async fn list_evening_tokens(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<EveningTokenRow>>> {
    let rows: Vec<EveningTokenRow> = sqlx::query_as(
        "SELECT id, user_id_1, user_id_2, booking_id, minted_at
         FROM evening_tokens
         WHERE user_id_1 = $1 OR user_id_2 = $1
         ORDER BY minted_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

/// Confirm a completed booking and mint an evening token for both parties.
///
/// Uses SELECT FOR UPDATE on the booking row to prevent double-minting if two
/// parties race to confirm simultaneously.
async fn confirm_evening_token(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(booking_id): Path<i32>,
) -> AppResult<Json<EveningTokenRow>> {
    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    // Lock the booking row — prevents concurrent double-mint.
    let booking: Option<BookingLock> = sqlx::query_as(
        "SELECT id, user_id_1, user_id_2, status
         FROM bookings
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(booking_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    let booking = booking.ok_or(AppError::NotFound)?;

    // Only participants may confirm.
    if booking.user_id_1 != user_id && booking.user_id_2 != user_id {
        return Err(AppError::Forbidden);
    }

    if booking.status != "completed" {
        return Err(AppError::bad_request("booking is not in completed status"));
    }

    // Idempotent — return existing token if already minted.
    let existing: Option<EveningTokenRow> = sqlx::query_as(
        "SELECT id, user_id_1, user_id_2, booking_id, minted_at
         FROM evening_tokens
         WHERE booking_id = $1",
    )
    .bind(booking_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    if let Some(row) = existing {
        tx.rollback().await.map_err(AppError::Db)?;
        return Ok(Json(row));
    }

    let row: EveningTokenRow = sqlx::query_as(
        "INSERT INTO evening_tokens (user_id_1, user_id_2, booking_id)
         VALUES ($1, $2, $3)
         RETURNING id, user_id_1, user_id_2, booking_id, minted_at",
    )
    .bind(booking.user_id_1)
    .bind(booking.user_id_2)
    .bind(booking_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    tx.commit().await.map_err(AppError::Db)?;

    Ok(Json(row))
}

#[derive(sqlx::FromRow)]
struct BookingLock {
    id:        i32,
    user_id_1: i32,
    user_id_2: i32,
    status:    String,
}

// ── Content tokens ────────────────────────────────────────────────────────────

async fn list_content_tokens(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<ContentTokenRow>>> {
    let rows: Vec<ContentTokenRow> = sqlx::query_as(
        "SELECT id, creator_id, owner_id, archetype, power, rarity, created_at
         FROM content_tokens
         WHERE owner_id = $1
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

/// Create a trade offer for a content token the caller owns.
async fn create_trade_offer(
    State(state): State<AppState>,
    RequireUser(from_user_id): RequireUser,
    Json(body): Json<TradeOfferBody>,
) -> AppResult<Json<TradeOfferRow>> {
    if body.to_user_id == from_user_id {
        return Err(AppError::bad_request("cannot trade with yourself"));
    }

    // Verify ownership without a transaction — the accept step re-verifies atomically.
    let is_owner: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM content_tokens WHERE id = $1 AND owner_id = $2)",
    )
    .bind(body.token_id)
    .bind(from_user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if !is_owner {
        return Err(AppError::Forbidden);
    }

    // Cancel any existing pending offer for this token so there is never more
    // than one live offer per token.
    sqlx::query(
        "UPDATE trade_offers SET status = 'cancelled'
         WHERE token_id = $1 AND from_user_id = $2 AND status = 'pending'",
    )
    .bind(body.token_id)
    .bind(from_user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    let row: TradeOfferRow = sqlx::query_as(
        "INSERT INTO trade_offers (token_id, from_user_id, to_user_id, status)
         VALUES ($1, $2, $3, 'pending')
         RETURNING id, token_id, from_user_id, to_user_id, status, created_at",
    )
    .bind(body.token_id)
    .bind(from_user_id)
    .bind(body.to_user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(row))
}

/// Accept a trade offer — atomically transfers token ownership.
async fn accept_trade(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(offer_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    // Lock the offer row.
    let offer: Option<TradeOfferLock> = sqlx::query_as(
        "SELECT id, token_id, from_user_id, to_user_id, status
         FROM trade_offers
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(offer_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    let offer = offer.ok_or(AppError::NotFound)?;

    if offer.to_user_id != user_id {
        return Err(AppError::Forbidden);
    }

    if offer.status != "pending" {
        return Err(AppError::bad_request("offer is no longer pending"));
    }

    // Re-verify the sender still owns the token (they may have traded it elsewhere).
    let still_owns: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM content_tokens WHERE id = $1 AND owner_id = $2)",
    )
    .bind(offer.token_id)
    .bind(offer.from_user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    if !still_owns {
        sqlx::query("UPDATE trade_offers SET status = 'cancelled' WHERE id = $1")
            .bind(offer_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Db)?;
        tx.commit().await.map_err(AppError::Db)?;
        return Err(AppError::bad_request("sender no longer owns this token"));
    }

    // Transfer ownership.
    sqlx::query("UPDATE content_tokens SET owner_id = $1 WHERE id = $2")
        .bind(user_id)
        .bind(offer.token_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Db)?;

    // Mark offer accepted and cancel any other pending offers for the same token.
    sqlx::query(
        "UPDATE trade_offers
         SET status = CASE WHEN id = $1 THEN 'accepted' ELSE 'cancelled' END
         WHERE token_id = $2 AND status = 'pending'",
    )
    .bind(offer_id)
    .bind(offer.token_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    tx.commit().await.map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({ "status": "accepted" })))
}

async fn decline_trade(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(offer_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        "UPDATE trade_offers SET status = 'declined'
         WHERE id = $1 AND to_user_id = $2 AND status = 'pending'",
    )
    .bind(offer_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "status": "declined" })))
}

#[derive(sqlx::FromRow)]
struct TradeOfferLock {
    id:           i32,
    token_id:     i32,
    from_user_id: i32,
    to_user_id:   i32,
    status:       String,
}

// ── Portrait tokens ───────────────────────────────────────────────────────────

async fn list_portrait_tokens(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<PortraitTokenRow>>> {
    let rows: Vec<PortraitTokenRow> = sqlx::query_as(
        "SELECT id, owner_id, creator_id, media_url, created_at
         FROM portrait_tokens
         WHERE owner_id = $1 OR creator_id = $1
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(rows))
}

/// Mint a portrait token — caller is the creator; subject_id becomes the owner.
async fn mint_portrait(
    State(state): State<AppState>,
    RequireUser(creator_id): RequireUser,
    Json(body): Json<MintPortraitBody>,
) -> AppResult<Json<PortraitTokenRow>> {
    let row: PortraitTokenRow = sqlx::query_as(
        "INSERT INTO portrait_tokens (owner_id, creator_id, media_url)
         VALUES ($1, $2, $3)
         RETURNING id, owner_id, creator_id, media_url, created_at",
    )
    .bind(body.subject_id)
    .bind(creator_id)
    .bind(&body.media_url)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(row))
}

/// Buy a portrait token — transfers ownership and pays a royalty to the creator.
///
/// Royalty split: 85% to seller (current owner), 15% to original creator.
/// Creates a Stripe PI for the full amount; royalty routing is handled at
/// webhook level when the PI is captured.
async fn buy_portrait(
    State(state): State<AppState>,
    RequireUser(buyer_id): RequireUser,
    Path(token_id): Path<i32>,
    Json(body): Json<BuyPortraitBody>,
) -> AppResult<Json<serde_json::Value>> {
    // Verify the token exists and the buyer is not the current owner.
    let token: Option<PortraitTokenRow> = sqlx::query_as(
        "SELECT id, owner_id, creator_id, media_url, created_at
         FROM portrait_tokens WHERE id = $1",
    )
    .bind(token_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;

    let token = token.ok_or(AppError::NotFound)?;

    if token.owner_id == buyer_id {
        return Err(AppError::bad_request("you already own this token"));
    }

    let price_cents = body.price_cents.max(100); // minimum CA$1

    let pi = state
        .stripe()
        .create_payment_intent(
            price_cents,
            "cad",
            None,
            &[
                ("type", "portrait_purchase"),
                ("token_id", &token_id.to_string()),
                ("buyer_id", &buyer_id.to_string()),
                ("seller_id", &token.owner_id.to_string()),
                ("creator_id", &token.creator_id.to_string()),
            ],
        )
        .await?;

    Ok(Json(serde_json::json!({
        "client_secret": pi.client_secret,
        "price_cents":   price_cents,
        "token_id":      token_id,
    })))
}

#[derive(serde::Deserialize)]
struct BuyPortraitBody {
    price_cents: i64,
}
