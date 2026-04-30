use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::{RequireDevice, RequireUser}, json::AppJson},
    types::OrderId,
};
use super::{repository, service, types::*};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/orders",                      post(create).get(list))
        .route("/api/orders/payment-intent",       post(payment_intent))
        .route("/api/orders/pay-with-balance",     post(pay_with_balance))
        .route("/api/orders/scan-collect",         post(scan_collect))
        .route("/api/orders/clip",                 post(clip))
        .route("/api/orders/{id}/confirm",          post(confirm))
        .route("/api/orders/{id}/rate",             post(rate))
        .route("/api/orders/{id}/receipt",          get(receipt))
        .route("/api/orders/{nfc_token}/collect",   post(device_collect))
}

// â”€â”€ Handlers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn create(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<CreateOrderBody>,
) -> AppResult<Json<CreateOrderResponse>> {
    Ok(Json(service::create_order(&state, user_id, body).await?))
}

async fn list(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<OrderRow>>> {
    Ok(Json(repository::list_for_user(&state.db, user_id).await?))
}

async fn payment_intent(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<PaymentIntentBody>,
) -> AppResult<Json<PaymentIntentResponse>> {
    Ok(Json(
        service::create_payment_intent(
            &state,
            user_id,
            body.variety_id,
            body.quantity,
            body.referral_code.as_deref(),
        )
        .await?,
    ))
}

async fn pay_with_balance(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<CreateOrderBody>,
) -> AppResult<Json<OrderRow>> {
    Ok(Json(service::pay_with_balance(&state, user_id, body).await?))
}

async fn confirm(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id): Path<OrderId>,
) -> AppResult<Json<OrderRow>> {
    Ok(Json(service::confirm_order(&state, order_id, user_id).await?))
}

async fn rate(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id): Path<OrderId>,
    AppJson(body): AppJson<RateOrderBody>,
) -> AppResult<Json<serde_json::Value>> {
    if !(1..=5).contains(&body.rating) {
        return Err(AppError::bad_request("rating must be 1â€“5"));
    }
    repository::set_rating(&state.db, order_id, user_id, body.rating).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn receipt(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(order_id): Path<OrderId>,
) -> AppResult<Json<OrderRow>> {
    let order = repository::find_by_id(&state.db, order_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if order.user_id != Some(user_id) {
        return Err(AppError::Forbidden);
    }
    Ok(Json(order))
}

async fn scan_collect(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<ScanCollectBody>,
) -> AppResult<Json<OrderRow>> {
    repository::collect_by_nfc(&state.db, &body.nfc_token, Some(user_id))
        .await?
        .ok_or_else(|| AppError::bad_request("order not found or not ready"))
        .map(Json)
}

async fn device_collect(
    State(state): State<AppState>,
    RequireDevice(device): RequireDevice,
    Path(nfc_token): Path<String>,
) -> AppResult<Json<OrderRow>> {
    if device.role != "employee" && device.role != "chocolatier" {
        return Err(AppError::Forbidden);
    }

    // Verify the device's owner is employed at the business that owns this order.
    // Without this check any employee device — regardless of which business it belongs
    // to — can collect orders from any other business.
    //
    // Check happens before the atomic collect so the order status is never mutated
    // by an unauthorised device. The subquery filters on status='ready' so it returns
    // false for both nonexistent and already-collected orders.
    let device_user = device.user_id.ok_or(AppError::Forbidden)?;
    let authorized: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM   orders o
             JOIN   locations          l  ON l.id           = o.location_id
             JOIN   employment_contracts ec ON ec.business_id = l.business_id
             WHERE  o.nfc_token  = $1
               AND  o.status     = 'ready'
               AND  ec.user_id   = $2
               AND  ec.status    = 'active'
         )",
    )
    .bind(&nfc_token)
    .bind(device_user)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if !authorized {
        return Err(AppError::Forbidden);
    }

    let order = repository::collect_by_nfc(&state.db, &nfc_token, None)
        .await?
        .ok_or_else(|| AppError::bad_request("order not found or not ready"))?;

    // Fire-and-forget push to customer.
    if let Some(uid) = order.user_id {
        let pool = state.db.clone();
        let http = state.http.clone();
        tokio::spawn(async move {
            if let Ok(Some((token,))) = sqlx::query_as::<_, (Option<String>,)>(
                "SELECT push_token FROM users WHERE id = $1"
            )
            .bind(uid)
            .fetch_optional(&pool)
            .await {
                if let Some(t) = token {
                    let _ = crate::integrations::expo_push::send(
                        &http,
                        crate::integrations::expo_push::PushMessage {
                            to:    &t,
                            title: Some("Your order is ready"),
                            body:  "Come collect your box",
                            ..Default::default()
                        },
                    ).await;
                }
            }
        });
    }

    Ok(Json(order))
}

/// App Clip guest order â€” creates a payment intent without requiring auth.
async fn clip(
    State(state): State<AppState>,
    AppJson(body): AppJson<CreateOrderBody>,
) -> AppResult<Json<serde_json::Value>> {
    let (price,): (i32,) = sqlx::query_as(
        "SELECT price_cents FROM varieties WHERE id = $1 AND active = true",
    )
    .bind(body.variety_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)?;

    let total_cents = price * body.quantity;
    let pi = state
        .stripe()
        .create_payment_intent(total_cents as i64, "cad", None, &[])
        .await?;

    Ok(Json(serde_json::json!({
        "client_secret": pi.client_secret,
        "total_cents":   total_cents,
    })))
}
