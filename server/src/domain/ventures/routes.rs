use axum::{
    extract::{Path, State},
    routing::get,
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
        .route("/api/ventures",              get(list).post(create))
        .route("/api/ventures/{id}",          get(find))
        .route("/api/ventures/{id}/members",  get(members))
        .route("/api/ventures/{id}/posts",    get(posts).post(create_post))
}

async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<VentureRow>>> {
    let rows: Vec<VentureRow> = sqlx::query_as(
        "SELECT id, name, description, ceo_type, ceo_user_id, status, fraise_cut, created_at
         FROM ventures
         WHERE status != 'archived'
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
) -> AppResult<Json<VentureRow>> {
    sqlx::query_as::<_, VentureRow>(
        "SELECT id, name, description, ceo_type, ceo_user_id, status, fraise_cut, created_at
         FROM ventures WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)
    .map(Json)
}

async fn create(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    AppJson(body): AppJson<CreateVentureBody>,
) -> AppResult<Json<VentureRow>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("name is required"));
    }

    let ceo_type = body.ceo_type.as_deref().unwrap_or("human");
    if !matches!(ceo_type, "human" | "dorotka") {
        return Err(AppError::bad_request("ceo_type must be 'human' or 'dorotka'"));
    }

    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    let venture: VentureRow = sqlx::query_as(
        "INSERT INTO ventures
             (name, description, ceo_type, ceo_user_id, created_by, status, fraise_cut)
         VALUES ($1, $2, $3, $4, $4, 'active', 0.15)
         RETURNING id, name, description, ceo_type, ceo_user_id, status, fraise_cut, created_at",
    )
    .bind(name)
    .bind(body.description.as_deref())
    .bind(ceo_type)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    // Auto-add the creator as owner.
    sqlx::query(
        "INSERT INTO venture_members (venture_id, user_id, role)
         VALUES ($1, $2, 'owner')
         ON CONFLICT DO NOTHING",
    )
    .bind(venture.id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    tx.commit().await.map_err(AppError::Db)?;

    Ok(Json(venture))
}

async fn members(
    State(state): State<AppState>,
    Path(venture_id): Path<i32>,
) -> AppResult<Json<Vec<VentureMemberRow>>> {
    let rows: Vec<VentureMemberRow> = sqlx::query_as(
        "SELECT vm.user_id, vm.venture_id, vm.role, vm.joined_at, u.display_name
         FROM venture_members vm
         JOIN users u ON u.id = vm.user_id
         WHERE vm.venture_id = $1
         ORDER BY vm.joined_at ASC",
    )
    .bind(venture_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn posts(
    State(state): State<AppState>,
    Path(venture_id): Path<i32>,
) -> AppResult<Json<Vec<VenturePostRow>>> {
    let rows: Vec<VenturePostRow> = sqlx::query_as(
        "SELECT id, venture_id, author_id, body, created_at
         FROM venture_posts
         WHERE venture_id = $1
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(venture_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;
    Ok(Json(rows))
}

async fn create_post(
    State(state): State<AppState>,
    RequireUser(user_id): RequireUser,
    Path(venture_id): Path<i32>,
    AppJson(body): AppJson<PostBody>,
) -> AppResult<Json<VenturePostRow>> {
    // Only members may post.
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1 FROM venture_members
             WHERE venture_id = $1 AND user_id = $2
         )",
    )
    .bind(venture_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    if !is_member {
        return Err(AppError::Forbidden);
    }

    if body.body.trim().is_empty() {
        return Err(AppError::bad_request("post body is required"));
    }

    let row: VenturePostRow = sqlx::query_as(
        "INSERT INTO venture_posts (venture_id, author_id, body)
         VALUES ($1, $2, $3)
         RETURNING id, venture_id, author_id, body, created_at",
    )
    .bind(venture_id)
    .bind(user_id)
    .bind(body.body.trim())
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(row))
}
