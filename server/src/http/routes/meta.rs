/// Platform-level routes that don't belong to any domain.
///
///   GET  /health                              — liveness probe
///   GET  /.well-known/apple-app-site-association — Universal Links
///   GET  /go?url=                             — privacy tracker hop
use axum::{
    extract::{Query, State},
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
/// Team ID and bundle ID are read from config so the binary is environment-portable.
async fn aasa(State(state): State<AppState>) -> Response {
    let team_id = state.cfg.apple_team_id.as_deref().unwrap_or("MISSING_TEAM_ID");
    let app_id  = format!("{team_id}.so.fraise.box");

    let body = serde_json::to_string(&json!({
        "applinks": {
            "apps": [],
            "details": [{ "appID": app_id, "paths": ["*"] }]
        },
        "webcredentials": {
            "apps": [app_id]
        }
    }))
    .unwrap_or_default();

    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

#[derive(Deserialize)]
struct GoQuery {
    url: Option<String>,
}

/// Privacy tracker hop — strips the Referer header so the destination site
/// cannot identify the originating Fraise page.
///
/// Only HTTPS URLs on known Fraise-owned or media-partner domains are forwarded.
/// Everything else is rejected to prevent this endpoint being used as an open
/// redirect by phishing campaigns.
async fn tracker_hop(Query(q): Query<GoQuery>) -> Response {
    let url = match q.url {
        Some(u) => u,
        None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "url required" }))).into_response(),
    };

    if !is_allowed_redirect(&url) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "url not permitted" }))).into_response();
    }

    Redirect::temporary(&url).into_response()
}

/// Allowlist for tracker_hop. Only HTTPS URLs whose host ends in a known
/// Fraise-controlled or trusted media-partner domain are forwarded.
fn is_allowed_redirect(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else { return false };

    if parsed.scheme() != "https" {
        return false;
    }

    let host = parsed.host_str().unwrap_or("");

    const ALLOWED_SUFFIXES: &[&str] = &[
        "fraise.box",
        "fraise.market",
        "fraise.skin",
        "loose.fish",
        "water.hiv",
        "cum.coffee",
        "cold.press",
    ];

    ALLOWED_SUFFIXES.iter().any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}
