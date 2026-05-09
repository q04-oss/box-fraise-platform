#![deny(clippy::disallowed_methods)] // Grade A item 5 — raw SQL belongs in repository.rs (clippy.toml)
//! Whisked menu routes — single public endpoint listing available drinks.

use axum::{extract::State, routing::get, Json, Router};

use crate::{app::AppState, error::AppResult};
use super::{service, types::WhiskedMenuItemRow};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/whisked/menu", get(list_menu))
}

/// GET /api/whisked/menu
///
/// Returns every available Whisked menu item ordered by `sort_order`. No
/// authentication required — the menu is part of the public marketing
/// surface and the iOS client fetches it before sign-in.
#[utoipa::path(
    get, path = "/api/whisked/menu", tag = "whisked",
    responses(
        (status = 200, description = "Available Whisked menu items", body = [WhiskedMenuItemRow]),
    ),
)]
pub async fn list_menu(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<WhiskedMenuItemRow>>> {
    let items = service::list_menu(&state.db).await?;
    Ok(Json(items))
}
