/// Platform-level routes that don't belong to any domain.
///
///   GET  /health                              — liveness probe
///   GET  /.well-known/apple-app-site-association — Universal Links
///   GET  /go?url=                             — privacy tracker hop
use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::app::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health",                                   get(health))
        .route("/.well-known/apple-app-site-association",   get(aasa))
        .route("/go",                                        get(tracker_hop))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// Apple App Site Association — drives Universal Links in the iOS app.
/// The JSON is served with the required content type.
async fn aasa() -> Response {
    let body = serde_json::to_string(&json!({
        "applinks": {
            "apps": [],
            "details": [
                {
                    "appID": "$(APPLE_TEAM_ID).so.fraise.box",
                    "paths": ["*"]
                }
            ]
        },
        "webcredentials": {
            "apps": ["$(APPLE_TEAM_ID).so.fraise.box"]
        }
    }))
    .unwrap_or_default();

    (
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

#[derive(Deserialize)]
struct GoQuery {
    url: Option<String>,
}

/// Tracker hop — redirects through the server so the destination never sees
/// the Referer header of the originating page.
async fn tracker_hop(Query(q): Query<GoQuery>) -> Response {
    match q.url {
        Some(url) if url.starts_with("https://") => Redirect::temporary(&url).into_response(),
        _ => (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid url" }))).into_response(),
    }
}
