use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use secrecy::ExposeSecret;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
    integrations::resend,
};
use super::types::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/popups",                           get(list))
        .route("/api/popups/{id}",                       get(find))
        .route("/api/popups/{id}/rsvp",                  post(rsvp).delete(cancel_rsvp))
        .route("/api/popups/{id}/rsvp-status",           get(rsvp_status))
        .route("/api/popups/{id}/checkin",               post(checkin))
        .route("/api/popups/{id}/nominations",           get(nominations))
        .route("/api/popups/{id}/nominate/{nominee_id}",  post(nominate))
}

// â”€â”€ List / find â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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

// â”€â”€ RSVP â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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
        // Paid event â€” create Stripe PI, RSVP pending payment.
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
        // Free event â€” confirm immediately.
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

// â”€â”€ Check-in â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn checkin(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(popup_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    // Must have a confirmed RSVP.
    let has_rsvp: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM popup_rsvps
             WHERE user_id = $1 AND business_id = $2 AND status = 'confirmed'
         )",
    )
    .bind(user_id)
    .bind(popup_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if !has_rsvp {
        return Err(AppError::bad_request("no confirmed RSVP for this event"));
    }

    sqlx::query(
        "INSERT INTO popup_checkins (user_id, business_id)
         VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(popup_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    // Add legitimacy event for attending.
    let _ = sqlx::query(
        "INSERT INTO legitimacy_events (user_id, event_type, weight)
         VALUES ($1, 'popup_checkin', 2)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(popup_id)
    .execute(&state.db)
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// â”€â”€ Nominations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

async fn nominations(
    State(state): State<AppState>,
    Path(popup_id): Path<i32>,
) -> AppResult<Json<Vec<NominationRow>>> {
    let rows: Vec<NominationRow> = sqlx::query_as(
        "SELECT id, popup_id, nominator_id, nominee_id, created_at
         FROM popup_nominations
         WHERE popup_id = $1
         ORDER BY created_at DESC",
    )
    .bind(popup_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn nominate(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path((popup_id, nominee_id)): Path<(i32, i32)>,
    AppJson(_): AppJson<NominateBody>,
) -> AppResult<Json<serde_json::Value>> {
    if user_id == nominee_id {
        return Err(AppError::bad_request("cannot nominate yourself"));
    }

    // Each user can nominate up to 3 people per popup.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM popup_nominations
         WHERE popup_id = $1 AND nominator_id = $2",
    )
    .bind(popup_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if count >= 3 {
        return Err(AppError::bad_request(
            "you have reached the nomination limit (3) for this event",
        ));
    }

    sqlx::query(
        "INSERT INTO popup_nominations (popup_id, nominator_id, nominee_id)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(popup_id)
    .bind(user_id)
    .bind(nominee_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if let Some(key) = state.cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned()) {
        let http = state.http.clone();
        let db   = state.db.clone();
        let uid  = user_id;
        tokio::spawn(async move {
            #[derive(sqlx::FromRow)]
            struct NomInfo {
                nominee_email:  Option<String>,
                nominator_name: Option<String>,
                event_name:     String,
            }

            let info: Option<NomInfo> = sqlx::query_as(
                "SELECT
                     (SELECT email FROM users WHERE id = $2)         AS nominee_email,
                     (SELECT display_name FROM users WHERE id = $1)  AS nominator_name,
                     (SELECT name FROM popup_events WHERE id = $3)   AS event_name",
            )
            .bind(uid)
            .bind(nominee_id)
            .bind(popup_id)
            .fetch_optional(&db)
            .await
            .unwrap_or(None);

            if let Some(NomInfo { nominee_email: Some(email), nominator_name, event_name }) = info {
                let nominator = nominator_name.as_deref().unwrap_or("Someone");
                let _ = resend::send_nomination(&http, &key, &email, &event_name, nominator).await;
            }
        });
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}
