// HMAC request signing middleware.
// Every iOS request carries:
//   X-Fraise-Client: ios
//   X-Fraise-Ts:     Unix timestamp (seconds)
//   X-Fraise-Sig:    HMAC-SHA256(method + path + ts + body, deviceKey) base64
//   X-Fraise-Attest-Key: App Attest key ID (after attestation)
//
// Key resolution: per-device key from device_attestations if attested,
// shared fallback (FRAISE_HMAC_SHARED_KEY) otherwise.
// Auth routes are excluded — Apple Sign In provides that guarantee.
// Replay prevention: in-process nonce cache (move to Redis for multi-instance).

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use ring::hmac as ring_hmac;
use serde_json::json;
use sqlx::PgPool;
use std::{
    collections::HashMap,
    env,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SKEW_SECS: u64 = 300;

pub type NonceCache = Arc<Mutex<HashMap<String, u64>>>;

pub fn new_nonce_cache() -> NonceCache {
    Arc::new(Mutex::new(HashMap::new()))
}

pub async fn validate(
    pool:        axum::extract::Extension<PgPool>,
    nonce_cache: axum::extract::Extension<NonceCache>,
    req:         Request,
    next:        Next,
) -> Response {
    let is_ios = req.headers()
        .get("x-fraise-client")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "ios")
        .unwrap_or(false);

    if !is_ios {
        return next.run(req).await;
    }

    // Auth routes: Apple Sign In is the gate; HMAC key isn't registered yet at sign-in.
    let path = req.uri().path();
    if path.starts_with("/api/auth/") {
        return next.run(req).await;
    }

    // Collect owned strings from headers before consuming the request
    let ts_str     = req.headers().get("x-fraise-ts").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let sig        = req.headers().get("x-fraise-sig").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let attest_key = req.headers().get("x-fraise-attest-key").and_then(|v| v.to_str().ok()).map(String::from);

    if ts_str.is_empty() || sig.is_empty() {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "missing signature" }))).into_response();
    }

    let ts: u64 = match ts_str.parse() {
        Ok(v) => v,
        Err(_) => return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "invalid timestamp" }))).into_response(),
    };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    if now.abs_diff(ts) > MAX_SKEW_SECS {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "request expired" }))).into_response();
    }

    // Replay check
    {
        let mut cache = nonce_cache.lock().unwrap();
        cache.retain(|_, exp| *exp > now);
        if cache.contains_key(&sig) {
            return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "request replayed" }))).into_response();
        }
    }

    // Collect body bytes for signing (axum requires consuming then reconstructing)
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "body too large" }))).into_response(),
    };

    // Resolve signing key
    let signing_key_bytes: Vec<u8> = if let Some(kid) = attest_key {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT hmac_key FROM device_attestations WHERE key_id = $1 AND hmac_key IS NOT NULL LIMIT 1"
        )
        .bind(&kid)
        .fetch_optional(&*pool)
        .await
        .unwrap_or(None);

        if let Some((key_b64,)) = row {
            use base64::{Engine, engine::general_purpose::STANDARD};
            STANDARD.decode(&key_b64).unwrap_or_else(|_| shared_key_bytes())
        } else {
            shared_key_bytes()
        }
    } else {
        shared_key_bytes()
    };

    // Compute expected HMAC
    let method    = parts.method.as_str();
    let path_qs   = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("");
    let mut msg   = format!("{}{}{}", method, path_qs, ts).into_bytes();
    msg.extend_from_slice(&body_bytes);

    let key      = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, &signing_key_bytes);
    let expected = ring_hmac::sign(&key, &msg);

    use base64::{Engine, engine::general_purpose::STANDARD};
    let expected_b64 = STANDARD.encode(expected.as_ref());

    if !constant_time_eq(expected_b64.as_bytes(), sig.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "invalid signature" }))).into_response();
    }

    // Cache nonce
    {
        let mut cache = nonce_cache.lock().unwrap();
        cache.insert(sig.clone(), now + MAX_SKEW_SECS);
    }

    let req = Request::from_parts(parts, Body::from(body_bytes));
    next.run(req).await
}

fn shared_key_bytes() -> Vec<u8> {
    env::var("FRAISE_HMAC_SHARED_KEY")
        .unwrap_or_else(|_| "fraise-request-signing-v1".into())
        .into_bytes()
}

// Constant-time comparison to prevent timing attacks on HMAC verification.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
