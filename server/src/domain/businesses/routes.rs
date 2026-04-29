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
use super::{repository, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/businesses",          get(list))
        .route("/api/businesses/:id",      get(find))
        .route("/api/businesses/:id/tip",  post(tip))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<BusinessRow>>> {
    Ok(Json(repository::list(&state.db).await?))
}

async fn find(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> AppResult<Json<BusinessRow>> {
    repository::find(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)
        .map(Json)
}

/// Tip the currently placed user at a business via Stripe.
/// The Stripe payment intent is created in manual-capture mode; the iOS
/// client confirms it and the server captures after confirmation.
async fn tip(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Path(business_id): Path<i32>,
    AppJson(body): AppJson<TipBody>,
) -> AppResult<Json<TipResponse>> {
    if body.amount_cents < 100 {
        return Err(AppError::bad_request("minimum tip is CA$1.00"));
    }
    if body.amount_cents > 100_000_00 {
        return Err(AppError::bad_request("tip exceeds maximum"));
    }

    // Verify there is a placed user to receive the tip.
    repository::placed_user_push_token(&state.db, business_id)
        .await?
        .ok_or_else(|| AppError::bad_request("no placed user at this business"))?;

    let pi = state
        .stripe()
        .create_payment_intent(
            body.amount_cents as i64,
            "cad",
            None,
            &[("type", "tip"), ("business_id", &business_id.to_string())],
        )
        .await?;

    Ok(Json(TipResponse {
        client_secret: pi.client_secret.unwrap_or_default(),
        total_cents:   body.amount_cents,
    }))
}
