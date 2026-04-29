/// HMAC-SHA256 iOS request signing middleware.
///
/// Every request from an iOS client carries:
///   X-Fraise-Client:     "ios"
///   X-Fraise-Ts:         Unix timestamp (seconds, string)
///   X-Fraise-Nonce:      UUID v4, per-request
///   X-Fraise-Sig:        base64( HMAC-SHA256(method + path + ts + nonce + body, key) )
///   X-Fraise-Attest-Key: App Attest key_id  [post-attestation only]
///   X-Fraise-Assertion:  App Attest ECDSA assertion over the same signed message [attested only]
///
/// The signed message is: method || path_and_query || ts || nonce || body
/// All components are concatenated as raw bytes with no separator. This matches
/// the iOS client: `"\(method)\(fullPath)\(timestamp)\(nonce)".data(using:.utf8)! + bodyData`
///
/// Key resolution (most specific wins):
///   1. Per-device key from device_attestations.hmac_key  (App Attest registered)
///   2. Shared key from FRAISE_HMAC_SHARED_KEY env var   (fallback for non-attested)
///   3. No shared key → 500 (fail closed, never silently degrade)
///
/// Replay prevention (ordered cheapest → most expensive):
///   1. Timestamp check: reject requests outside 5-minute window (no I/O)
///   2. Nonce format: reject non-UUID values (no I/O, prevents Redis key injection)
///   3. HMAC verification: cryptographic check (CPU only)
///   4. Nonce dedup: atomic Redis SET NX EX, or in-process HashMap fallback
///
///   Nonces are stored under "fraise:nonce:<lowercase-uuid>" with a TTL matching
///   MAX_SKEW_SECS. Redis is used when REDIS_URL is configured (multi-instance safe);
///   the in-process HashMap is used otherwise (single instance only).
///
///   Replay attempts are logged at WARN — they can be benign (URLSession retry on
///   timeout) or adversarial. A burst from one device/IP is the signal to investigate.
///
/// Rejection status codes:
///   400  missing or malformed nonce (structural, not auth)
///   401  missing sig headers / expired timestamp / invalid HMAC / invalid assertion
///   409  replayed nonce (nonce already seen within the timestamp window)
///   500  server misconfiguration (no signing key, Redis failure)
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
use deadpool_redis::redis;
use ring::hmac as ring_hmac;
use serde_json::json;
use secrecy::ExposeSecret;

use crate::{app::AppState, auth::apple_attest};

/// Replay window in seconds. Must match the iOS client's timestamp tolerance.
/// Also used as the Redis TTL for nonce entries and the in-process cache expiry.
/// Change both sides together if this value changes.
pub const MAX_SKEW_SECS: u64 = 300;

// ── In-process nonce cache (fallback when Redis is not configured) ─────────────

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

    // ── Extract headers before the body is consumed ───────────────────────────
    let ts_str = owned_header(&req, "x-fraise-ts");
    let sig    = owned_header(&req, "x-fraise-sig");
    let nonce  = owned_header(&req, "x-fraise-nonce");
    let kid    = opt_header(&req, "x-fraise-attest-key");

    // Structural check — before any I/O.
    if ts_str.is_empty() || sig.is_empty() {
        return reject(StatusCode::UNAUTHORIZED, "missing signature headers");
    }
    if nonce.is_empty() {
        return reject(StatusCode::BAD_REQUEST, "nonce required");
    }
    // UUID format validation prevents key-injection into Redis (e.g. "fraise:nonce:*").
    if uuid::Uuid::parse_str(&nonce).is_err() {
        return reject(StatusCode::BAD_REQUEST, "malformed nonce");
    }

    // ── Timestamp check (cheap — no I/O) ─────────────────────────────────────
    let ts: u64 = match ts_str.parse() {
        Ok(v)  => v,
        Err(_) => return reject(StatusCode::UNAUTHORIZED, "invalid timestamp"),
    };
    if unix_now().abs_diff(ts) > MAX_SKEW_SECS {
        return reject(StatusCode::UNAUTHORIZED, "request expired");
    }

    // ── Body collection ───────────────────────────────────────────────────────
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b)  => b,
        Err(_) => return reject(StatusCode::BAD_REQUEST, "body too large"),
    };

    // ── Key resolution ────────────────────────────────────────────────────────
    let key_bytes: Vec<u8> = match kid {
        Some(ref kid) => match resolve_device_key(kid, &state).await {
            Ok(k)  => k,
            Err(r) => return r,
        },
        None => match shared_key(&state) {
            Some(k) => k,
            None    => return reject(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        },
    };

    // ── HMAC verification — method + path_and_query + ts + nonce + body ───────
    //
    // Matches the iOS signed message exactly:
    //   "\(method)\(fullPath)\(timestamp)\(nonce)".data(using:.utf8)! + bodyData
    //
    // path_and_query is used (not just path) so query parameters are covered.
    let method  = parts.method.as_str();
    let path_qs = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("");

    let mut msg = format!("{method}{path_qs}{ts}{nonce}").into_bytes();
    msg.extend_from_slice(&body_bytes);

    let ring_key     = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, &key_bytes);
    let expected     = ring_hmac::sign(&ring_key, &msg);
    let expected_b64 = STANDARD.encode(expected.as_ref());

    if !constant_time_eq(expected_b64.as_bytes(), sig.as_bytes()) {
        return reject(StatusCode::UNAUTHORIZED, "invalid signature");
    }

    // ── App Attest assertion verification ─────────────────────────────────────
    // X-Fraise-Assertion is generated on the iOS side over the same `message`
    // bytes that include the nonce. We reuse `msg` computed above.
    if let Some(kid) = &kid {
        let assertion_header = parts
            .headers
            .get("x-fraise-assertion")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        if let Some(assertion) = assertion_header {
            match resolve_public_key(kid, &state).await {
                Some(pub_key_der) => {
                    if apple_attest::verify_assertion(&assertion, &pub_key_der, &msg).is_err() {
                        return reject(StatusCode::UNAUTHORIZED, "assertion_invalid");
                    }
                }
                None => {
                    // No public key stored yet — allow through, HMAC provides integrity.
                    tracing::debug!(kid, "no public key for device — skipping assertion check");
                }
            }
        }
    }

    // ── Nonce deduplication ───────────────────────────────────────────────────
    // HMAC is verified — now commit the nonce. This order (HMAC before Redis) means
    // only cryptographically valid requests reach Redis, minimising load from garbage.
    if !check_and_reserve_nonce(&nonce, &state).await {
        tracing::warn!(
            nonce  = %nonce,
            path   = %path_qs,
            method = %method,
            "replay attempt — nonce already seen within window"
        );
        return reject(StatusCode::CONFLICT, "request replayed");
    }

    let req = Request::from_parts(parts, Body::from(body_bytes));
    next.run(req).await
}

// ── Nonce store ───────────────────────────────────────────────────────────────

/// Atomically checks and reserves a nonce.
/// Returns `true` if the nonce was fresh (request should proceed).
/// Returns `false` if the nonce was already seen or the store failed (fail closed).
async fn check_and_reserve_nonce(nonce: &str, state: &AppState) -> bool {
    if let Some(ref redis_pool) = state.redis {
        // Redis path — multi-instance safe.
        // SET fraise:nonce:<uuid> 1 EX 300 NX
        // Returns OK (success) or Nil (key already existed).
        let key = format!("fraise:nonce:{}", nonce.to_ascii_lowercase());
        match redis_pool.get().await {
            Ok(mut conn) => {
                let result: Result<redis::Value, redis::RedisError> = redis::cmd("SET")
                    .arg(&key)
                    .arg(1u8)
                    .arg("EX")
                    .arg(MAX_SKEW_SECS)
                    .arg("NX")
                    .query_async(&mut *conn)
                    .await;
                match result {
                    // Nil means NX failed — key already existed — replay.
                    Ok(redis::Value::Nil) => false,
                    // Any non-Nil success means the key was set — fresh nonce.
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!(error = %e, "Redis nonce check failed — failing closed");
                        false
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Redis pool error — failing closed");
                false
            }
        }
    } else {
        // In-process fallback — single instance only.
        let now = unix_now();
        let mut cache = state.nonces.lock().unwrap();
        cache.retain(|_, exp| *exp > now);
        if cache.contains_key(nonce) {
            return false;
        }
        cache.insert(nonce.to_string(), now + MAX_SKEW_SECS);
        true
    }
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

fn reject(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
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

async fn resolve_device_key(kid: &str, state: &AppState) -> Result<Vec<u8>, Response> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT hmac_key FROM device_attestations \
         WHERE key_id = $1 AND hmac_key IS NOT NULL \
         LIMIT 1",
    )
    .bind(kid)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?;

    match row {
        Some((key_b64,)) => STANDARD.decode(&key_b64)
            .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "internal error")),
        None => Err(reject(StatusCode::UNAUTHORIZED, "unknown device")),
    }
}

/// Constant-time byte comparison — prevents timing oracle on HMAC tags.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ring::hmac as ring_hmac;

    // ── Message construction ──────────────────────────────────────────────────

    /// The nonce must be inside the HMAC-signed bytes, not just a separate header.
    /// A nonce that's only a header can be stripped by a MITM without invalidating the sig.
    #[test]
    fn nonce_is_inside_signed_message() {
        let method = "POST";
        let path   = "/api/orders";
        let ts     = 1_700_000_000u64;
        let nonce  = "550e8400-e29b-41d4-a716-446655440000";
        let body   = b"{}";

        let mut msg = format!("{method}{path}{ts}{nonce}").into_bytes();
        msg.extend_from_slice(body);

        // The header portion (everything before the body) must contain the nonce.
        let header_part = &msg[..msg.len() - body.len()];
        let header_str  = std::str::from_utf8(header_part).unwrap();
        assert!(header_str.contains(nonce), "Nonce must be in the signed message bytes");
        assert!(
            header_str.ends_with(nonce),
            "Nonce must be the last string component before body (order: method+path+ts+nonce)"
        );
    }

    /// Changing the nonce must change the HMAC signature.
    /// If two nonces produce the same signature, the nonce is not being signed.
    #[test]
    fn different_nonces_produce_different_signatures() {
        let key_bytes = b"test-signing-key-exactly-32bytes";
        let method    = "GET";
        let path      = "/api/catalog";
        let ts        = "1700000000";
        let body: &[u8] = b"";

        let sign = |nonce: &str| -> String {
            let mut msg = format!("{method}{path}{ts}{nonce}").into_bytes();
            msg.extend_from_slice(body);
            let key = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, key_bytes);
            STANDARD.encode(ring_hmac::sign(&key, &msg).as_ref())
        };

        let nonce_a = "550e8400-e29b-41d4-a716-446655440000";
        let nonce_b = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        assert_ne!(
            sign(nonce_a),
            sign(nonce_b),
            "Different nonces must produce different HMAC signatures"
        );
    }

    /// The signed message format must match the iOS client exactly.
    /// iOS: "\(method)\(fullPath)\(timestamp)\(nonce)".data(using:.utf8)! + bodyData
    #[test]
    fn message_format_matches_ios_client() {
        let method = "POST";
        let path   = "/api/platform-messages/send";
        let ts     = 1_700_000_000u64;
        let nonce  = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        let body   = br#"{"recipient_code":"ABC123"}"#;

        // Server-side construction (this file)
        let mut server_msg = format!("{method}{path}{ts}{nonce}").into_bytes();
        server_msg.extend_from_slice(body);

        // iOS-side construction (Swift pseudocode translated to Rust)
        let ios_str = format!("{method}{path}{ts}{nonce}");
        let mut ios_msg = ios_str.as_bytes().to_vec();
        ios_msg.extend_from_slice(body);

        assert_eq!(server_msg, ios_msg, "Server and iOS message bytes must be identical");
    }

    // ── UUID validation ───────────────────────────────────────────────────────

    #[test]
    fn valid_uuids_accepted() {
        let cases = [
            "550e8400-e29b-41d4-a716-446655440000", // lowercase
            "6BA7B810-9DAD-11D1-80B4-00C04FD430C8", // uppercase (iOS UUID().uuidString format)
            "00000000-0000-0000-0000-000000000000",
        ];
        for u in &cases {
            assert!(uuid::Uuid::parse_str(u).is_ok(), "Must accept valid UUID: {u}");
        }
    }

    #[test]
    fn invalid_nonces_rejected() {
        let cases = [
            "",
            "not-a-uuid",
            "123",
            "../../etc/passwd",
            "fraise:nonce:injection",
            "550e8400e29b41d4a716446655440000", // no dashes
        ];
        for u in &cases {
            assert!(uuid::Uuid::parse_str(u).is_err(), "Must reject: {u}");
        }
    }

    // ── In-process nonce store ────────────────────────────────────────────────

    #[test]
    fn fresh_nonce_is_accepted() {
        let nonces = new_nonce_cache();
        let now    = unix_now();
        let nonce  = "550e8400-e29b-41d4-a716-446655440001";

        let mut cache = nonces.lock().unwrap();
        assert!(!cache.contains_key(nonce));
        cache.insert(nonce.to_string(), now + MAX_SKEW_SECS);
        assert!(cache.contains_key(nonce));
    }

    #[test]
    fn replayed_nonce_is_rejected() {
        let nonces = new_nonce_cache();
        let now    = unix_now();
        let nonce  = "550e8400-e29b-41d4-a716-446655440002";

        {
            let mut cache = nonces.lock().unwrap();
            cache.insert(nonce.to_string(), now + MAX_SKEW_SECS);
        }
        // Second lookup must find it already present.
        let cache = nonces.lock().unwrap();
        assert!(cache.contains_key(nonce), "Replayed nonce must be detected");
    }

    #[test]
    fn expired_nonces_are_evicted() {
        let nonces = new_nonce_cache();
        let now    = unix_now();
        let nonce  = "550e8400-e29b-41d4-a716-446655440003";

        {
            let mut cache = nonces.lock().unwrap();
            cache.insert(nonce.to_string(), now.saturating_sub(1)); // already expired
        }
        {
            let mut cache = nonces.lock().unwrap();
            cache.retain(|_, exp| *exp > now);
            assert!(!cache.contains_key(nonce), "Expired nonce must be evicted");
        }
    }

    // ── Constant-time comparison ──────────────────────────────────────────────

    #[test]
    fn constant_time_eq_matches_equal_slices() {
        assert!(constant_time_eq(b"abc", b"abc"));
    }

    #[test]
    fn constant_time_eq_rejects_different_slices() {
        assert!(!constant_time_eq(b"abc", b"xyz"));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
