use axum::{
    extract::State,
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
        .route("/api/memberships/payment-intent", post(payment_intent))
        .route("/api/memberships/me",             get(my_membership))
        .route("/api/members",                    get(list_members))
}

// -- Handlers -----------------------------------------------------------------

async fn payment_intent(
    State(state):     State<AppState>,
    RequireUser(uid): RequireUser,
    AppJson(body):    AppJson<PaymentIntentBody>,
) -> AppResult<Json<PaymentIntentResponse>> {
    let tier    = body.tier.trim().to_lowercase();
    let uid_str = i32::from(uid).to_string();

    // Hard gate -- only three tiers may be purchased via Stripe.
    // Higher tiers require a manual invoice; this cannot be bypassed by the client.
    if !STRIPE_PAYABLE_TIERS.contains(&tier.as_str()) {
        return Err(AppError::bad_request(
            "this membership tier requires a manual invoice -- contact us directly",
        ));
    }

    let amount_cents = tier_amount_cents(&tier)
        .ok_or_else(|| AppError::bad_request("unknown membership tier"))?;

    // user_id must be in metadata so the webhook can activate the membership.
    // Without it complete_membership silently no-ops -- the user is charged
    // but the membership row is never written.
    let pi = state
        .stripe()
        .create_payment_intent(
            amount_cents,
            "cad",
            None,
            &[("type", "membership"), ("tier", &tier), ("user_id", &uid_str)],
        )
        .await?;

    // Pre-create a pending membership row anchored to this PI.
    // The webhook resolves user_id and tier from this row, not from metadata.
    sqlx::query(
        "INSERT INTO memberships
             (user_id, tier, status, amount_cents, stripe_payment_intent_id)
         VALUES ($1, $2, 'pending', $3, $4)
         ON CONFLICT (user_id) DO UPDATE
         SET tier = EXCLUDED.tier,
             status = 'pending',
             amount_cents = EXCLUDED.amount_cents,
             stripe_payment_intent_id = EXCLUDED.stripe_payment_intent_id",
    )
    .bind(uid)
    .bind(&tier)
    .bind(amount_cents as i32)
    .bind(&pi.id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(PaymentIntentResponse {
        client_secret: pi.client_secret.unwrap_or_default(),
        amount_cents,
        tier,
    }))
}

async fn my_membership(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<serde_json::Value>> {
    let membership = repository::find_for_user(&state.db, user_id).await?;
    Ok(Json(serde_json::json!({ "membership": membership })))
}

async fn list_members(
    State(state):    State<AppState>,
    RequireUser(_):  RequireUser,
) -> AppResult<Json<Vec<MemberRow>>> {
    Ok(Json(repository::list_active_members(&state.db).await?))
}
