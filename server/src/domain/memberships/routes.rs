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
        .route("/api/memberships/payment-intent", post(payment_intent))
        .route("/api/memberships/me",             get(my_membership))
        .route("/api/memberships/waitlist",        post(join_waitlist))
        .route("/api/members",                    get(list_members))
        .route("/api/fund/contribute/:user_id",   post(contribute))
        .route("/api/fund/:user_id/contributors", get(contributors))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn payment_intent(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    AppJson(body): AppJson<PaymentIntentBody>,
) -> AppResult<Json<PaymentIntentResponse>> {
    let tier = body.tier.trim().to_lowercase();

    // Hard gate — only three tiers may be purchased via Stripe.
    // Higher tiers require a manual invoice; this cannot be bypassed by the client.
    if !STRIPE_PAYABLE_TIERS.contains(&tier.as_str()) {
        return Err(AppError::bad_request(
            "this membership tier requires a manual invoice — contact us directly",
        ));
    }

    let amount_cents = tier_amount_cents(&tier)
        .ok_or_else(|| AppError::bad_request("unknown membership tier"))?;

    let pi = state
        .stripe()
        .create_payment_intent(
            amount_cents,
            "cad",
            None,
            &[("type", "membership"), ("tier", &tier)],
        )
        .await?;

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

async fn join_waitlist(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<WaitlistBody>,
) -> AppResult<Json<serde_json::Value>> {
    let tier = body.tier.trim().to_lowercase();
    if tier_amount_cents(&tier).is_none() {
        return Err(AppError::bad_request("unknown membership tier"));
    }
    repository::join_waitlist(&state.db, user_id, &tier).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_members(State(state): State<AppState>) -> AppResult<Json<Vec<MemberRow>>> {
    Ok(Json(repository::list_active_members(&state.db).await?))
}

async fn contribute(
    State(state): State<AppState>,
    RequireUser(from_user_id): RequireUser,
    Path(recipient_id): Path<i32>,
    AppJson(body): AppJson<ContributeBody>,
) -> AppResult<Json<serde_json::Value>> {
    if body.amount_cents < 100 {
        return Err(AppError::bad_request("minimum contribution is CA$1.00"));
    }

    // Atomically deduct from contributor's platform_credit_cents.
    let success = sqlx::query(
        "UPDATE users
         SET platform_credit_cents = platform_credit_cents - $1
         WHERE id = $2 AND platform_credit_cents >= $1",
    )
    .bind(body.amount_cents)
    .bind(from_user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?
    .rows_affected()
        > 0;

    if !success {
        return Err(AppError::bad_request("insufficient platform credit"));
    }

    // Credit the recipient's membership fund.
    sqlx::query(
        "UPDATE membership_funds
         SET balance_cents = balance_cents + $1
         WHERE user_id = $2",
    )
    .bind(body.amount_cents)
    .bind(recipient_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    // Record the transaction.
    sqlx::query(
        "INSERT INTO fund_contributions (user_id, recipient_id, amount_cents)
         VALUES ($1, $2, $3)",
    )
    .bind(from_user_id)
    .bind(recipient_id)
    .bind(body.amount_cents)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn contributors(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<Vec<serde_json::Value>>> {
    Ok(Json(
        repository::list_fund_contributors(&state.db, user_id).await?,
    ))
}
