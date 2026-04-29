/// HMAC-SHA256 iOS request signing middleware.
///
/// Every request from an iOS client carries:
///   X-Fraise-Client:     "ios"
///   X-Fraise-Ts:         Unix timestamp (seconds)
///   X-Fraise-Sig:        base64( HMAC-SHA256(method + path + ts + body, key) )
///   X-Fraise-Attest-Key: App Attest key_id  [post-attestation only]
///
/// Key resolution (most specific wins):
///   1. Per-device key stored in device_attestations.hmac_key  (App Attest)
///   2. Shared key from FRAISE_HMAC_SHARED_KEY env var          (fallback)
///   3. Absent shared key → 500 (fail closed, never silently degrade)
///
/// Replay prevention:
///   Signatures are cached in-process for MAX_SKEW_SECS after first use.
///   Move the cache to Redis before running multiple server instances.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::hmac as ring_hmac;
use serde_json::json;

use secrecy::ExposeSecret;

use crate::{app::AppState, auth::apple_attest};

const MAX_SKEW_SECS: u64 = 300; // 5-minute replay window

// ── Nonce cache ───────────────────────────────────────────────────────────────

pub type NonceCache = Arc<Mutex<HashMap<String, u64>>>;

pub fn new_nonce_cache() -> NonceCache {
    Arc::new(Mutex::new(HashMap::new()))
}

// ── Middleware ────────────────────────────────────────────────────────────────

pub async fn validate(
    State(state): State<AppState>,
    req:          Request,
    next:         Next,
) -> Response {
    // Only iOS clients carry the signing headers.
    let is_ios = req
        .headers()
        .get("x-fraise-client")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "ios")
        .unwrap_or(false);

    if !is_ios {
        return next.run(req).await;
    }

    // Auth routes are exempt — the device key doesn't exist yet at sign-in.
    if req.uri().path().starts_with("/api/auth/") {
        return next.run(req).await;
    }

    // ── Collect headers before the request body is consumed ──────────────────
    let ts_str = owned_header(&req, "x-fraise-ts");
    let sig    = owned_header(&req, "x-fraise-sig");
    let kid    = opt_header(&req,   "x-fraise-attest-key");

    if ts_str.is_empty() || sig.is_empty() {
        return reject("missing signature");
    }

    // ── Timestamp validation ──────────────────────────────────────────────────
    let ts: u64 = match ts_str.parse() {
        Ok(v)  => v,
        Err(_) => return reject("invalid timestamp"),
    };

    let now = unix_now();
    if now.abs_diff(ts) > MAX_SKEW_SECS {
        return reject("request expired");
    }

    // ── Replay check + nonce reservation (single lock — atomic) ──────────────
    {
        let mut cache = state.nonces.lock().unwrap();
        cache.retain(|_, exp| *exp > now);
        if cache.contains_key(&sig) {
            return reject("request replayed");
        }
        // Reserve the slot immediately; removed if verification fails below.
        cache.insert(sig.clone(), now + MAX_SKEW_SECS);
    }

    // ── Body collection ───────────────────────────────────────────────────────
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b)  => b,
        Err(_) => {
            state.nonces.lock().unwrap().remove(&sig);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "body too large" })),
            )
                .into_response();
        }
    };

    // ── Key resolution ────────────────────────────────────────────────────────
    let key_bytes: Vec<u8> = match kid {
        Some(ref kid) => match resolve_device_key(kid, &state).await {
            Ok(k)  => k,
            Err(r) => {
                state.nonces.lock().unwrap().remove(&sig);
                return r;
            }
        },
        None => match shared_key(&state) {
            Some(k) => k,
            None    => {
                state.nonces.lock().unwrap().remove(&sig);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "internal error" })),
                )
                    .into_response();
            }
        },
    };

    // ── HMAC computation ──────────────────────────────────────────────────────
    let method  = parts.method.as_str();
    let path_qs = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("");

    let mut msg = format!("{method}{path_qs}{ts}").into_bytes();
    msg.extend_from_slice(&body_bytes);

    let ring_key  = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, &key_bytes);
    let expected  = ring_hmac::sign(&ring_key, &msg);
    let expected_b64 = STANDARD.encode(expected.as_ref());

    if !constant_time_eq(expected_b64.as_bytes(), sig.as_bytes()) {
        state.nonces.lock().unwrap().remove(&sig);
        return reject("invalid signature");
    }

    // ── App Attest assertion verification ─────────────────────────────────────
    // X-Fraise-Assertion is present on every request from an attested device.
    // If we have a stored public key for this device, verify the assertion.
    // This provides a second trust layer: HMAC proves the signing key matches;
    // the assertion proves the binary is unmodified on genuine Apple hardware.
    if let Some(kid) = &kid {
        let assertion_header = parts
            .headers
            .get("x-fraise-assertion")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        if let Some(assertion) = assertion_header {
            match resolve_public_key(kid, &state).await {
                Some(pub_key_der) => {
                    // Reuse the same message bytes computed for HMAC above.
                    let mut msg_for_attest = format!("{method}{path_qs}{ts}").into_bytes();
                    msg_for_attest.extend_from_slice(&body_bytes);

                    if apple_attest::verify_assertion(&assertion, &pub_key_der, &msg_for_attest)
                        .is_err()
                    {
                        state.nonces.lock().unwrap().remove(&sig);
                        return reject("assertion_invalid");
                    }
                }
                None => {
                    // No public key stored yet (device attested before this column existed).
                    // Allow through — HMAC provides sufficient integrity for legacy devices.
                    tracing::debug!(kid, "no public key for device — skipping assertion check");
                }
            }
        }
    }

    // Nonce was already reserved; verification passed — forward the request.
    let req = Request::from_parts(parts, Body::from(body_bytes));
    next.run(req).await
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn owned_header(req: &Request, name: &str) -> String {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned()
}

fn opt_header(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

fn reject(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": msg }))).into_response()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn shared_key(state: &AppState) -> Option<Vec<u8>> {
    state.cfg.hmac_shared_key.as_ref().map(|k| k.expose_secret().as_bytes().to_vec())
}

/// Look up the per-device HMAC key stored at attestation time.
/// Returns the raw key bytes or an error response to forward to the client.
async fn resolve_device_key(kid: &str, state: &AppState) -> Result<Vec<u8>, Response> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT hmac_key FROM device_attestations \
         WHERE key_id = $1 AND hmac_key IS NOT NULL \
         LIMIT 1",
    )
    .bind(kid)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal error" })),
        )
            .into_response()
    })?;

    match row {
        Some((key_b64,)) => STANDARD.decode(&key_b64).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal error" })),
            )
                .into_response()
        }),
        None => Err(
            (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unknown device" }))).into_response(),
        ),
    }
}

/// Constant-time byte-slice comparison — prevents timing oracle on HMAC tags.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Fetch the DER-encoded public key for an attested device.
/// Returns `None` if the device has no public key stored (pre-attestation or legacy).
async fn resolve_public_key(kid: &str, state: &AppState) -> Option<Vec<u8>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT public_key FROM device_attestations \
         WHERE key_id = $1 AND public_key IS NOT NULL \
         LIMIT 1",
    )
    .bind(kid)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    row.and_then(|(b64,)| STANDARD.decode(&b64).ok())
}
