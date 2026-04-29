use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireUser,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/search", get(search))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState>,
    RequireUser(_): RequireUser,
    Query(params): Query<SearchQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let q = params.q.trim();
    if q.is_empty() || q.len() > 100 {
        return Err(AppError::bad_request("q must be 1–100 characters"));
    }
    let pattern = format!("%{}%", q);

    // Users — verified, non-banned, by display_name. Limit 8.
    #[derive(sqlx::FromRow)]
    struct UserRow {
        id:           i32,
        display_name: Option<String>,
        portrait_url: Option<String>,
        user_code:    Option<String>,
    }
    let users: Vec<UserRow> = sqlx::query_as(
        "SELECT id, display_name, portrait_url, user_code
         FROM users
         WHERE display_name ILIKE $1
           AND verified = true
           AND banned   = false
         ORDER BY display_name
         LIMIT 8",
    )
    .bind(&pattern)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    // Varieties — active, by name. Limit 5.
    #[derive(sqlx::FromRow)]
    struct VarietyRow {
        id:          i32,
        name:        String,
        image_url:   Option<String>,
        price_cents: i32,
    }
    let varieties: Vec<VarietyRow> = sqlx::query_as(
        "SELECT id, name, image_url, price_cents
         FROM varieties
         WHERE name ILIKE $1 AND active = true
         ORDER BY name
         LIMIT 5",
    )
    .bind(&pattern)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    // Businesses — active, by name. Limit 5.
    #[derive(sqlx::FromRow)]
    struct BizRow {
        id:      i32,
        name:    String,
        address: Option<String>,
    }
    let businesses: Vec<BizRow> = sqlx::query_as(
        "SELECT id, name, address
         FROM businesses
         WHERE name ILIKE $1 AND active = true
         ORDER BY name
         LIMIT 5",
    )
    .bind(&pattern)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(serde_json::json!({
        "users": users.iter().map(|u| serde_json::json!({
            "id":           u.id,
            "display_name": u.display_name,
            "portrait_url": u.portrait_url,
            "user_code":    u.user_code,
        })).collect::<Vec<_>>(),
        "varieties": varieties.iter().map(|v| serde_json::json!({
            "id":          v.id,
            "name":        v.name,
            "image_url":   v.image_url,
            "price_cents": v.price_cents,
        })).collect::<Vec<_>>(),
        "businesses": businesses.iter().map(|b| serde_json::json!({
            "id":      b.id,
            "name":    b.name,
            "address": b.address,
        })).collect::<Vec<_>>(),
    })))
}
