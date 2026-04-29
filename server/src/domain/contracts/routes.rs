use axum::{
    extract::{Path, State},
    routing::{get, patch},
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
        .route("/api/contracts/pending",       get(pending))
        .route("/api/contracts/active",        get(active))
        .route("/api/contracts/{id}/accept",    patch(accept))
        .route("/api/contracts/{id}/decline",   patch(decline))
}

async fn pending(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Vec<ContractRow>>> {
    let rows: Vec<ContractRow> = sqlx::query_as(
        "SELECT id, business_id, user_id, status, note, start_date, end_date, created_at
         FROM employment_contracts
         WHERE user_id = $1 AND status = 'pending'
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn active(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
) -> AppResult<Json<Option<ContractRow>>> {
    let row: Option<ContractRow> = sqlx::query_as(
        "SELECT id, business_id, user_id, status, note, start_date, end_date, created_at
         FROM employment_contracts
         WHERE user_id = $1 AND status = 'active'
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(row))
}

async fn accept(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(contract_id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        "UPDATE employment_contracts
         SET status = 'active'
         WHERE id = $1 AND user_id = $2 AND status = 'pending'",
    )
    .bind(contract_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request("contract not found or already responded"));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn decline(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(contract_id): Path<i32>,
    AppJson(body): AppJson<RespondBody>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        "UPDATE employment_contracts
         SET status = 'declined', note = $3
         WHERE id = $1 AND user_id = $2 AND status = 'pending'",
    )
    .bind(contract_id)
    .bind(user_id)
    .bind(body.note.as_deref())
    .execute(&state.db)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::bad_request("contract not found or already responded"));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
