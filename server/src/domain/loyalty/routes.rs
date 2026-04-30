/// Loyalty endpoints.
///
/// GET  /api/businesses/:id/loyalty              — balance (RequireUser, rate-limited)
/// GET  /api/businesses/:id/loyalty/history      — event history (RequireUser)
/// GET  /api/businesses/:id/loyalty/qr-token     — issue stamp token (RequireUser)
/// POST /api/businesses/:id/loyalty/stamp        — record steep via app scanner (RequireStaff)
/// GET  /stamp                                   — HTML stamp page; camera-scan fallback
///
/// NFC cup sticker endpoints:
/// POST /api/staff/nfc/activate   — staff scans companion QR; opens 2h activation window
/// POST /api/nfc/redeem           — app calls this when customer taps sticker
/// GET  /nfc/{uuid}               — Universal Link target; fallback HTML if app not installed
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    http::extractors::{
        auth::{RequireStaff, RequireUser},
        json::AppJson,
    },
};
use super::{service, types::StampBody};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/businesses/{id}/loyalty",         get(balance))
        .route("/api/businesses/{id}/loyalty/history", get(history))
        // NFC sticker routes
        .route("/api/staff/nfc/activate", post(nfc_activate))
        .route("/api/nfc/redeem",         post(nfc_redeem))
        .route("/nfc/{uuid}",             get(nfc_tap))
        .route("/api/businesses/{id}/loyalty/qr-token",get(qr_token))
        .route("/api/businesses/{id}/loyalty/stamp",   post(stamp))
        // HTML fallback — opened in phone browser after camera scan
        .route("/stamp", get(stamp_html))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn balance(
    State(state):      State<AppState>,
    Path(business_id): Path<i32>,
    RequireUser(uid):  RequireUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> AppResult<Json<super::types::LoyaltyBalance>> {
    Ok(Json(service::get_balance(&state, uid, business_id, Some(addr.ip())).await?))
}

#[derive(Deserialize)]
struct HistoryParams {
    limit:  Option<i64>,
    offset: Option<i64>,
}

async fn history(
    State(state):      State<AppState>,
    Path(business_id): Path<i32>,
    RequireUser(uid):  RequireUser,
    Query(params):     Query<HistoryParams>,
) -> AppResult<Json<Vec<super::types::LoyaltyEventRow>>> {
    Ok(Json(service::get_history(
        &state, uid, business_id,
        params.limit.unwrap_or(20),
        params.offset.unwrap_or(0),
    ).await?))
}

async fn qr_token(
    State(state):      State<AppState>,
    Path(business_id): Path<i32>,
    RequireUser(uid):  RequireUser,
) -> AppResult<Json<super::types::QrTokenResponse>> {
    Ok(Json(service::issue_qr_token(&state, uid, business_id).await?))
}

async fn stamp(
    State(state):         State<AppState>,
    Path(business_id):    Path<i32>,
    RequireStaff(claims): RequireStaff,
    ConnectInfo(addr):    ConnectInfo<SocketAddr>,
    AppJson(body):        AppJson<StampBody>,
) -> AppResult<Json<super::types::StampResult>> {
    // The path business_id and the JWT's business_id must match.
    // This prevents a staff member from using the /businesses/B/stamp endpoint
    // with a JWT issued for business A.
    if claims.business_id != business_id {
        return Err(AppError::Forbidden);
    }

    Ok(Json(service::stamp_via_qr(
        &state,
        claims.user_id,
        claims.business_id,
        &body.qr_token,
        Some(addr.ip()),
    ).await?))
}

// ── HTML stamp page ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StampHtmlParams {
    /// QR token from the customer's screen.
    t: Option<String>,
    /// business_id — must match what's encoded in the token.
    b: Option<i32>,
}

async fn stamp_html(
    State(state):      State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params):     Query<StampHtmlParams>,
) -> Response {
    let (Some(token), Some(bid)) = (params.t, params.b) else {
        return stamp_page(StampPageState::Error("missing token or business parameter")).into_response();
    };

    match service::stamp_via_html(&state, &token, bid, Some(addr.ip())).await {
        Ok(result)                   => stamp_page(StampPageState::Ok(result)).into_response(),
        Err(AppError::Unauthorized)  => stamp_page(StampPageState::Error("QR code expired or already used")).into_response(),
        Err(AppError::Forbidden)     => stamp_page(StampPageState::Error("this code is not valid at this location")).into_response(),
        Err(AppError::Conflict(_))   => stamp_page(StampPageState::AlreadyStamped).into_response(),
        Err(AppError::Unprocessable(m)) => stamp_page(StampPageState::Error(&m)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "stamp_html failed");
            stamp_page(StampPageState::Error("something went wrong — please try again")).into_response()
        }
    }
}

enum StampPageState<'a> {
    Ok(super::types::StampResult),
    AlreadyStamped,
    Error(&'a str),
}

fn stamp_page(state: StampPageState) -> Html<String> {
    let (icon, heading, body_html) = match &state {
        StampPageState::Ok(r) => {
            let reward_line = if r.reward_available {
                format!("<p class=\"reward\">🎁 Reward available: {}</p>", r.reward_description)
            } else {
                String::new()
            };
            (
                "✓",
                "Steep recorded",
                format!(
                    "<p class=\"name\">{}</p>\
                     <p class=\"balance\">{} steeps</p>\
                     {reward_line}",
                    r.customer_name, r.new_balance,
                ),
            )
        }
        StampPageState::AlreadyStamped => (
            "✓",
            "Already stamped",
            "<p>This code has already been redeemed.</p>".to_string(),
        ),
        StampPageState::Error(msg) => (
            "✗",
            "Could not record steep",
            format!("<p class=\"error\">{msg}</p>"),
        ),
    };

    Html(format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Box Fraise · {heading}</title>
<style>
  *{{box-sizing:border-box;margin:0;padding:0}}
  body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
       background:#F7F5F2;display:flex;align-items:center;justify-content:center;
       min-height:100vh}}
  .card{{background:#fff;border-radius:20px;padding:48px 32px;max-width:360px;
        width:90%;text-align:center;box-shadow:0 4px 24px rgba(0,0,0,.07)}}
  .icon{{font-size:3rem;margin-bottom:12px}}
  h1{{font-size:1.3rem;font-weight:600;color:#1C1C1E;margin-bottom:20px}}
  .name{{font-size:1.1rem;font-weight:500;color:#1C1C1E;margin-bottom:4px}}
  .balance{{font-size:2.5rem;font-weight:700;color:#1C1C1E;margin:12px 0}}
  .reward{{font-size:.9rem;color:#4CAF50;font-weight:500;margin-top:12px}}
  .error{{font-size:.9rem;color:#C0392B;margin-top:8px}}
  p{{font-size:.875rem;color:#8E8E93;line-height:1.5;margin-top:8px}}
</style>
</head>
<body>
<div class="card">
  <div class="icon">{icon}</div>
  <h1>{heading}</h1>
  {body_html}
</div>
</body>
</html>"#))
}


// ── NFC sticker handlers ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NfcActivateBody { sticker_uuid: String }

#[derive(Serialize)]
struct NfcActivateResponse { ok: bool, expires_in_secs: u64 }

async fn nfc_activate(
    State(state):         State<AppState>,
    RequireStaff(claims): RequireStaff,
    AppJson(body):        AppJson<NfcActivateBody>,
) -> AppResult<Json<NfcActivateResponse>> {
    service::activate_nfc_sticker(
        &state,
        claims.user_id,
        claims.business_id,
        &body.sticker_uuid,
    ).await?;
    Ok(Json(NfcActivateResponse { ok: true, expires_in_secs: 7_200 }))
}

#[derive(Deserialize)]
#[derive(Deserialize)]
struct NfcRedeemBody { sticker_uuid: String }

#[derive(Serialize)]
struct NfcRedeemResponse {
    business_id:        i32,
    customer_name:      String,
    new_balance:        i64,
    reward_available:   bool,
    reward_description: String,
}

async fn nfc_redeem(
    State(state):      State<AppState>,
    RequireUser(uid):  RequireUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    AppJson(body):     AppJson<NfcRedeemBody>,
) -> AppResult<Json<NfcRedeemResponse>> {
    let result = service::redeem_nfc_sticker(
        &state, uid, &body.sticker_uuid, Some(addr.ip())
    ).await?;

    Ok(Json(NfcRedeemResponse {
        business_id:        result.business_id,
        customer_name:      result.customer_name,
        new_balance:        result.new_balance,
        reward_available:   result.reward_available,
        reward_description: result.reward_description,
    }))
}

/// Universal Link target — called when a customer taps the NFC sticker on a
/// device that does not have the Box Fraise app installed.
/// Devices with the app never reach this endpoint — iOS intercepts the URL
/// and opens the app directly via Universal Links.
async fn nfc_tap(Path(uuid): Path<String>) -> Response {
    // The app is not installed. Show a download prompt.
    // The UUID is preserved in the URL so the app can redeem immediately after install.
    Html(format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>box fraise</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
     background:#F7F5F2;display:flex;align-items:center;justify-content:center;
     min-height:100svh;text-align:center}}
.card{{background:#fff;border-radius:20px;padding:48px 28px;width:90%;max-width:340px;
      box-shadow:0 4px 24px rgba(0,0,0,.07)}}
h1{{font-size:1.3rem;font-weight:600;color:#1C1C1E;margin:16px 0 8px}}
p{{font-size:.875rem;color:#8E8E93;line-height:1.5;margin-bottom:24px}}
a{{display:block;background:#1C1C1E;color:#F7F5F2;text-decoration:none;
   padding:14px;border-radius:12px;font-size:.9rem;font-weight:500}}
</style>
</head>
<body>
<div class="card">
  <div style="font-size:2.5rem">☕</div>
  <h1>earn a steep</h1>
  <p>download box fraise to collect loyalty steeps from your drinks.</p>
  <a href="https://apps.apple.com/app/box-fraise/id0000000000">get the app</a>
</div>
</body>
</html>"#)).into_response()
}
