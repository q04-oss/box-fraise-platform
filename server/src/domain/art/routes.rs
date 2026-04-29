use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
};
use super::types::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/art",                      get(gallery))
        .route("/api/art/pitch",                post(pitch))
        .route("/api/art/auctions",             get(auctions))
        .route("/api/art/auctions/:id/bid",     post(bid))
        .route("/api/art/auctions/:id/bids",    get(auction_bids))
}

async fn gallery(State(state): State<AppState>) -> AppResult<Json<Vec<ArtworkRow>>> {
    let rows: Vec<ArtworkRow> = sqlx::query_as(
        "SELECT id, pitch_id, title, media_url, description, status, created_at
         FROM artworks
         WHERE status IN ('posted','acquired','auctioned')
         ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn pitch(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<PitchBody>,
) -> AppResult<Json<serde_json::Value>> {
    if body.title.trim().is_empty() {
        return Err(AppError::bad_request("title is required"));
    }

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO art_pitches (user_id, title, description, media_url, status)
         VALUES ($1, $2, $3, $4, 'submitted')
         RETURNING id",
    )
    .bind(user_id)
    .bind(body.title.trim())
    .bind(body.description.as_deref())
    .bind(body.media_url.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({ "id": id, "status": "submitted" })))
}

async fn auctions(State(state): State<AppState>) -> AppResult<Json<Vec<AuctionRow>>> {
    let rows: Vec<AuctionRow> = sqlx::query_as(
        "SELECT id, artwork_id, reserve_cents, starts_at, ends_at, status
         FROM art_auctions
         WHERE status = 'active'
         ORDER BY ends_at ASC NULLS LAST",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

/// Place a bid on an active auction.
///
/// Security: the entire read-check-insert is wrapped in a transaction with
/// a row-level lock (`FOR UPDATE`) on the auction so concurrent bids cannot
/// both win the same increment check.
async fn bid(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(auction_id): Path<i32>,
    AppJson(body): AppJson<BidBody>,
) -> AppResult<Json<BidRow>> {
    if body.amount_cents < 1 {
        return Err(AppError::bad_request("bid must be positive"));
    }

    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    // Lock the auction row to prevent races.
    #[derive(sqlx::FromRow)]
    struct AuctionLock { status: String, reserve_cents: Option<i64> }

    let auction: AuctionLock = sqlx::query_as(
        "SELECT status, reserve_cents FROM art_auctions
         WHERE id = $1 FOR UPDATE",
    )
    .bind(auction_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)?;

    if auction.status != "active" {
        return Err(AppError::bad_request("auction is not active"));
    }

    // Check bid exceeds current maximum.
    let current_max: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(amount_cents) FROM art_bids WHERE auction_id = $1",
    )
    .bind(auction_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    let floor = current_max
        .or(auction.reserve_cents)
        .unwrap_or(0);

    if body.amount_cents <= floor {
        return Err(AppError::bad_request(
            "bid must exceed the current highest bid",
        ));
    }

    // Create Stripe payment intent (manual capture — captured only if bid wins).
    let pi = state
        .stripe()
        .create_payment_intent(
            body.amount_cents,
            "cad",
            None,
            &[("type", "art_bid"), ("auction_id", &auction_id.to_string())],
        )
        .await?;

    let row: BidRow = sqlx::query_as(
        "INSERT INTO art_bids
             (auction_id, user_id, amount_cents, stripe_payment_intent_id)
         VALUES ($1, $2, $3, $4)
         RETURNING id, auction_id, user_id, amount_cents,
                   stripe_payment_intent_id, created_at",
    )
    .bind(auction_id)
    .bind(user_id)
    .bind(body.amount_cents)
    .bind(pi.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    tx.commit().await.map_err(AppError::Db)?;

    Ok(Json(row))
}

async fn auction_bids(
    State(state): State<AppState>,
    Path(auction_id): Path<i32>,
) -> AppResult<Json<Vec<BidRow>>> {
    let rows: Vec<BidRow> = sqlx::query_as(
        "SELECT id, auction_id, user_id, amount_cents,
                stripe_payment_intent_id, created_at
         FROM art_bids
         WHERE auction_id = $1
         ORDER BY amount_cents DESC",
    )
    .bind(auction_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}
