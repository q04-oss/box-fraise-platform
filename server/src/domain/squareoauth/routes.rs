/// Square OAuth endpoints.
///
/// GET  /api/square/oauth/connect    — RequireStaff; redirects to Square consent page
/// GET  /api/square/oauth/callback   — public; Square redirects here after approval
///
/// The connect endpoint requires staff authentication because only the business
/// operator should be able to connect a Square account. The staff JWT carries
/// the business_id so the state token stored in Redis is scoped correctly.
///
/// The callback is intentionally public — Square's redirect does not carry
/// any auth headers. Security is provided by the CSRF state token (GETDEL).
use axum::{
    extract::{ConnectInfo, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::net::SocketAddr;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::auth::RequireStaff,
};
use super::service;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/square/oauth/connect",  get(connect))
        .route("/api/square/oauth/callback", get(callback))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn connect(
    State(state):      State<AppState>,
    RequireStaff(claims): RequireStaff,
) -> AppResult<Redirect> {
    let url = service::connect_url(&state, claims.business_id).await?;
    Ok(Redirect::to(&url))
}

#[derive(Deserialize)]
struct CallbackParams {
    code:  Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(state):       State<AppState>,
    ConnectInfo(addr):  ConnectInfo<SocketAddr>,
    Query(params):      Query<CallbackParams>,
) -> Response {
    // Square sends error=access_denied when the merchant clicks "Deny".
    if let Some(err) = params.error {
        return oauth_result_page(false, &format!("Square declined: {err}"));
    }

    let code  = match params.code  { Some(c) => c, None => return oauth_result_page(false, "missing code parameter") };
    let state_token = match params.state { Some(s) => s, None => return oauth_result_page(false, "missing state parameter") };

    match service::handle_callback(&state, &code, &state_token, Some(addr.ip()), crate::integrations::square::BASE).await {
        Ok(_)  => oauth_result_page(true,  ""),
        Err(AppError::Unauthorized) => oauth_result_page(false, "invalid or expired session — please try again"),
        Err(e) => {
            tracing::error!(error = %e, "Square OAuth callback failed");
            oauth_result_page(false, "connection failed — please try again or contact support")
        }
    }
}

// ── HTML result page ──────────────────────────────────────────────────────────

fn oauth_result_page(ok: bool, message: &str) -> Response {
    let (title, heading, body_content) = if ok {
        (
            "Square connected",
            "Square connected",
            "<p>Online orders will now appear on your Square POS automatically.<br>\
             You can close this tab.</p>".to_string(),
        )
    } else {
        (
            "Connection failed",
            "Connection failed",
            format!(
                "<p>Something went wrong connecting your Square account.</p>\
                 <p><code>{message}</code></p>",
            ),
        )
    };

    let icon  = if ok { "🔔" } else { "⚠️" };
    let html = format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Box Fraise · {title}</title>
<style>
  body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
       background:#F7F5F2;display:flex;align-items:center;justify-content:center;
       min-height:100vh;margin:0}}
  .card{{background:#fff;border-radius:16px;padding:40px 32px;max-width:380px;
        text-align:center;box-shadow:0 2px 16px rgba(0,0,0,.06)}}
  h1{{font-size:1.25rem;font-weight:600;color:#1C1C1E;margin:16px 0 8px}}
  p{{font-size:.875rem;color:#8E8E93;line-height:1.5;margin:0 0 8px}}
  code{{font-size:.75rem;color:#c0392b;background:#fdf0ee;padding:2px 6px;border-radius:4px}}
</style>
</head>
<body>
<div class="card">
  <div style="font-size:2.5rem">{icon}</div>
  <h1>{heading}</h1>
  {body_content}
</div>
</body>
</html>"#);

    axum::response::Html(html).into_response()
}
