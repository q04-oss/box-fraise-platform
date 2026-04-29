/// NFC connection endpoints.
///
/// The iOS app taps two devices together and calls /connect with a shared
/// pairing token generated at the same location. The server records a
/// bidirectional connection between the two users.
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{auth::RequireUser, json::AppJson},
    types::UserId,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/nfc/contacts",  get(contacts))
        .route("/api/nfc/connect",   post(connect))
}

#[derive(Debug, Deserialize)]
struct ConnectBody {
    /// The other user's ID (resolved by the app from the NFC tag).
    other_user_id: UserId,
    location_id:   Option<i32>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct ContactRow {
    user_id:      UserId,
    display_name: Option<String>,
    portrait_url: Option<String>,
    connected_at: NaiveDateTime,
}

async fn contacts(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<ContactRow>>> {
    let rows: Vec<ContactRow> = sqlx::query_as(
        "SELECT u.id AS user_id, u.display_name, u.portrait_url,
                nc.created_at AS connected_at
         FROM nfc_connections nc
         JOIN users u ON u.id = CASE
             WHEN nc.user_id = $1 THEN nc.connected_user_id
             ELSE nc.user_id
         END
         WHERE nc.user_id = $1 OR nc.connected_user_id = $1
         ORDER BY nc.created_at DESC
         LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn connect(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<ConnectBody>,
) -> AppResult<Json<serde_json::Value>> {
    if user_id == body.other_user_id {
        return Err(AppError::bad_request("cannot NFC-connect with yourself"));
    }

    // Store both directions so queries don't need OR on both columns.
    for (a, b) in [(user_id, body.other_user_id), (body.other_user_id, user_id)] {
        sqlx::query(
            "INSERT INTO nfc_connections (user_id, connected_user_id, location_id)
             VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(a)
        .bind(b)
        .bind(body.location_id)
        .execute(&state.db)
        .await
        .map_err(AppError::Db)?;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}
