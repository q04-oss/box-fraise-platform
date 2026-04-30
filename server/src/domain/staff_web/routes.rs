/// Staff web app — a PWA served from the API binary.
///
/// Three pages:
///   GET  /staff          → redirect to /staff/scan if session cookie present,
///                          otherwise /staff/login
///   GET  /staff/login    → login form (PIN + location ID)
///   POST /staff/login    → validates PIN, sets HttpOnly session cookie, redirects
///   GET  /staff/scan     → camera scanner (requires session cookie)
///   POST /staff/stamp    → called by scanner JS; validates session, stamps
///   GET  /staff/manifest → PWA manifest for homescreen install
///
/// Auth model: the session cookie holds the StaffClaims JWT, set HttpOnly so
/// JavaScript cannot read it. The /staff/stamp endpoint reads the cookie
/// server-side, verifies the JWT, and calls the loyalty service directly.
/// JavaScript only sees the stamp result (customer name + balance) — never the token.
///
/// CSRF protection: SameSite=Strict on the cookie means the browser will not
/// send it on cross-origin requests. Combined with the staff JWT being
/// HttpOnly, there is no viable CSRF attack surface.
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_cookies::{Cookie, Cookies};

use crate::{
    app::AppState,
    domain::{auth, loyalty},
    error::AppError,
};

const COOKIE_NAME: &str = "staff_session";
const COOKIE_MAX_AGE_SECS: i64 = 8 * 3600; // one shift

// Compiled into the binary at build time. The only runtime placeholder is
// __BUSINESS_ID__ — injected via str::replace so the JS file uses normal
// { } syntax with no Rust format-string escaping.
static SCAN_JS: &str = include_str!("scan.js");

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/staff",          get(root))
        .route("/staff/login",    get(login_page).post(login_submit))
        .route("/staff/scan",     get(scan_page))
        .route("/staff/stamp",    post(stamp))
        .route("/staff/manifest", get(manifest))
}

// ── Root ──────────────────────────────────────────────────────────────────────

async fn root(State(state): State<AppState>, cookies: Cookies) -> Redirect {
    if extract_claims(&cookies, &state.cfg).is_some() {
        Redirect::to("/staff/scan")
    } else {
        Redirect::to("/staff/login")
    }
}

// ── Login ─────────────────────────────────────────────────────────────────────

async fn login_page() -> Html<String> {
    Html(page(
        "staff login",
        r#"
        <form method="POST" action="/staff/login" class="card">
            <h1>box fraise</h1>
            <p class="sub">staff login</p>
            <label>
                <span>location</span>
                <input name="location_id" type="number" inputmode="numeric"
                       placeholder="location ID" required autofocus>
            </label>
            <label>
                <span>pin</span>
                <input name="pin" type="password" inputmode="numeric"
                       placeholder="staff PIN" required>
            </label>
            <button type="submit">sign in</button>
        </form>
        "#,
    ))
}

#[derive(Deserialize)]
struct LoginForm {
    location_id: i32,
    pin:         String,
}

async fn login_submit(
    State(state): State<AppState>,
    cookies:      Cookies,
    Form(form):   Form<LoginForm>,
) -> Response {
    let result = auth::service::staff_login(
        &state,
        &form.pin,
        form.location_id,
        None,
    ).await;

    match result {
        Ok(resp) => {
            let mut cookie = Cookie::new(COOKIE_NAME, resp.token);
            cookie.set_http_only(true);
            cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
            cookie.set_secure(true);
            cookie.set_path("/staff");
            cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(COOKIE_MAX_AGE_SECS));
            cookies.add(cookie);
            Redirect::to("/staff/scan").into_response()
        }
        Err(_) => Html(page(
            "staff login",
            r#"
            <form method="POST" action="/staff/login" class="card">
                <h1>box fraise</h1>
                <p class="sub">staff login</p>
                <p class="error">incorrect PIN or location — try again</p>
                <label>
                    <span>location</span>
                    <input name="location_id" type="number" inputmode="numeric"
                           placeholder="location ID" required autofocus>
                </label>
                <label>
                    <span>pin</span>
                    <input name="pin" type="password" inputmode="numeric"
                           placeholder="staff PIN" required>
                </label>
                <button type="submit">sign in</button>
            </form>
            "#,
        )).into_response()
    }
}

// ── Scan ──────────────────────────────────────────────────────────────────────

async fn scan_page(State(state): State<AppState>, cookies: Cookies) -> Response {
    let Some(claims) = extract_claims(&cookies, &state.cfg) else {
        return Redirect::to("/staff/login").into_response();
    };

    let nonce  = generate_nonce();
    let script = SCAN_JS.replace("__BUSINESS_ID__", &claims.business_id.to_string());
    let html   = build_scan_html(&nonce, &script);
    let csp    = build_scan_csp(&nonce);

    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_SECURITY_POLICY, csp)
        .body(Body::from(html))
        .unwrap()
}

/// 16 bytes of CSPRNG entropy, base64-encoded → 24-char nonce.
/// UUIDs are for uniqueness; nonces need unpredictability — use OsRng.
fn generate_nonce() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

fn build_scan_html(nonce: &str, script: &str) -> String {
    let card = r#"<div class="card scan-card" id="card">
            <p class="sub" id="mode-label">stamp mode</p>
            <div class="mode-toggle">
                <button id="btn-stamp"    class="mode active">stamp customer</button>
                <button id="btn-activate" class="mode"       >activate cup</button>
            </div>
            <div class="video-wrap">
                <video id="video" autoplay playsinline muted></video>
                <div class="viewfinder"></div>
            </div>
            <p id="status" class="status">point camera at customer QR</p>
        </div>"#;
    // script content is already a resolved String — no {{ escaping needed.
    page("scan", &format!("{card}<script nonce=\"{nonce}\">{script}</script>"))
}

fn build_scan_csp(nonce: &str) -> String {
    // unsafe-inline is dropped; the nonce covers the inline scan script.
    // https://cdn.jsdelivr.net is still required for jsqr (external, URL-matched).
    format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}' https://cdn.jsdelivr.net; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         connect-src 'self'; \
         media-src 'self' blob:; \
         frame-ancestors 'none'"
    )
}

// ── Stamp ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StampBody {
    qr_token:    String,
    business_id: i32,
}

#[derive(Serialize)]
struct StampOk {
    customer_name:     String,
    new_balance:       i64,
    reward_available:  bool,
    reward_description: String,
}

#[derive(Serialize)]
struct StampErr {
    error: String,
}

async fn stamp(
    State(state): State<AppState>,
    cookies:      Cookies,
    Json(body):   Json<StampBody>,
) -> Response {
    // Authenticate from cookie — JS never touches the token.
    let claims = match extract_claims(&cookies, &state.cfg) {
        Some(c) => c,
        None    => return (
            StatusCode::UNAUTHORIZED,
            Json(StampErr { error: "session expired — please log in again".into() }),
        ).into_response(),
    };

    // The business_id the JS parsed from the QR must match the staff's session.
    if claims.business_id != body.business_id {
        return (
            StatusCode::FORBIDDEN,
            Json(StampErr { error: "this code is not valid at your location".into() }),
        ).into_response();
    }

    let user_id = claims.user_id;

    match loyalty::service::stamp_via_qr(
        &state,
        user_id,
        claims.business_id,
        &body.qr_token,
        None,
    ).await {
        Ok(result) => (StatusCode::OK, Json(StampOk {
            customer_name:      result.customer_name,
            new_balance:        result.new_balance,
            reward_available:   result.reward_available,
            reward_description: result.reward_description,
        })).into_response(),

        Err(AppError::Unauthorized) => (
            StatusCode::UNAUTHORIZED,
            Json(StampErr { error: "QR code expired or already used".into() }),
        ).into_response(),

        Err(AppError::Forbidden) => (
            StatusCode::FORBIDDEN,
            Json(StampErr { error: "this code is not valid at your location".into() }),
        ).into_response(),

        Err(AppError::Conflict(_)) => (
            StatusCode::CONFLICT,
            Json(StampErr { error: "already stamped".into() }),
        ).into_response(),

        Err(e) => {
            tracing::error!(error = %e, "staff_web stamp failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StampErr { error: "something went wrong — try again".into() }),
            ).into_response()
        }
    }
}

// ── PWA manifest ──────────────────────────────────────────────────────────────

async fn manifest() -> Response {
    let json = serde_json::json!({
        "name":             "Box Fraise Staff",
        "short_name":       "Staff",
        "start_url":        "/staff",
        "display":          "standalone",
        "background_color": "#F7F5F2",
        "theme_color":      "#1C1C1E",
        "icons": [{
            "src":   "/staff/icon.png",
            "sizes": "192x192",
            "type":  "image/png"
        }]
    });
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("application/manifest+json"))],
        json.to_string(),
    ).into_response()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extract_claims(
    cookies: &Cookies,
    cfg:     &crate::config::Config,
) -> Option<crate::auth::staff::StaffClaims> {
    let token = cookies.get(COOKIE_NAME)?.value().to_owned();
    crate::auth::staff::verify_staff_token(&token, cfg)
}

/// Shared HTML shell — minimal, mobile-first, installable as PWA.
fn page(title: &str, content: &str) -> String {
    format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
<meta name="apple-mobile-web-app-title" content="Staff">
<link rel="manifest" href="/staff/manifest">
<title>{title} · box fraise</title>
<!-- jsqr 1.4.0 — QR decoder for iOS Safari fallback (BarcodeDetector unavailable on iOS).
     Version and SRI hash are pinned; update both together if upgrading. -->
<script src="https://cdn.jsdelivr.net/npm/jsqr@1.4.0/dist/jsQR.js"
        integrity="sha384-b5Ya4Bq3qCyz39m2ISh+4DxjAIljdeFwK/BsXLuj9gugaNwAcj/ia15fxNZL9Nlx"
        crossorigin="anonymous"></script>
<style>
*{{box-sizing:border-box;margin:0;padding:0;-webkit-tap-highlight-color:transparent}}
:root{{
  --bg:    #F7F5F2;
  --card:  #FFFFFF;
  --text:  #1C1C1E;
  --muted: #8E8E93;
  --border:#E5E1DA;
  --green: #4CAF50;
  --red:   #C0392B;
}}
body{{
  font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
  background:var(--bg);min-height:100svh;
  display:flex;align-items:center;justify-content:center;
  padding:env(safe-area-inset-top) env(safe-area-inset-right)
          env(safe-area-inset-bottom) env(safe-area-inset-left);
}}
.card{{
  background:var(--card);border-radius:20px;
  padding:40px 28px;width:90%;max-width:360px;
  display:flex;flex-direction:column;gap:16px;
}}
.scan-card{{max-width:420px;padding:28px 20px}}
h1{{font-size:1.5rem;font-weight:600;color:var(--text);letter-spacing:-.02em}}
.sub{{font-size:.75rem;color:var(--muted);letter-spacing:.1em;text-transform:uppercase;
      font-variant-numeric:tabular-nums}}
label{{display:flex;flex-direction:column;gap:6px}}
label span{{font-size:.75rem;color:var(--muted);text-transform:uppercase;letter-spacing:.08em}}
input{{
  background:var(--bg);border:1px solid var(--border);border-radius:10px;
  padding:13px 14px;font-size:1rem;color:var(--text);width:100%;
  -webkit-appearance:none;
}}
input:focus{{outline:none;border-color:var(--text)}}
button{{
  background:var(--text);color:var(--bg);border:none;border-radius:12px;
  padding:14px;font-size:.9rem;font-weight:500;cursor:pointer;
  -webkit-appearance:none;
}}
.error{{font-size:.85rem;color:var(--red);padding:8px 12px;
        background:#fdf0ee;border-radius:8px}}
.status{{font-size:.85rem;color:var(--muted);text-align:center}}
.video-wrap{{
  position:relative;border-radius:14px;overflow:hidden;
  background:#000;aspect-ratio:1;width:100%;
}}
video{{width:100%;height:100%;object-fit:cover;display:block}}
.viewfinder{{
  position:absolute;inset:20%;border:2px solid rgba(255,255,255,.6);
  border-radius:8px;pointer-events:none;
}}
.result{{display:flex;flex-direction:column;align-items:center;gap:12px;padding:20px 0}}
.icon{{font-size:3rem}}
.name{{font-size:1.1rem;font-weight:500;color:var(--text)}}
.balance{{font-size:2.5rem;font-weight:700;color:var(--text)}}
.reward{{font-size:.85rem;color:var(--green);font-weight:500}}
.ok .icon{{color:var(--green)}}
.err .icon{{color:var(--red)}}
.mode-toggle{{display:flex;gap:6px;width:100%}}
.mode{{flex:1;background:var(--bg);border:1px solid var(--border);border-radius:8px;
       padding:9px 6px;font-size:.78rem;cursor:pointer;color:var(--muted);
       -webkit-appearance:none;transition:background .15s,color .15s}}
.mode.active{{background:var(--text);color:var(--bg);border-color:var(--text)}}
</style>
</head>
<body>
{content}
</body>
</html>"#)
}
