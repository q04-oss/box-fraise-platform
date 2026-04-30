use chrono::{Duration, Utc};
use deadpool_redis::redis;
use uuid::Uuid;

use crate::{
    audit,
    app::AppState,
    error::{AppError, AppResult},
    types::UserId,
};
use super::{repository, types::*};

// Redis key namespacing --------------------------------------------------------
// fraise:stamp:{uuid}           → "{business_id}:{user_id}"   (QR token)
// fraise:rate:loyalty-bal:{uid} → counter, EX 60              (balance rate limit)
// fraise:rate:loyalty-stamp:{bid} → counter, EX 60            (stamp rate limit)

// Lua script: atomically validate business ownership and consume a QR stamp token.
//
// Threat prevented: with plain GETDEL the token is destroyed before the
// business_id check, so a rogue staff member from business B could burn a
// customer's token intended for business A. This script only DELetes if
// the stored business_id matches ARGV[1], leaving the token intact on mismatch.
//
// KEYS[1]: fraise:stamp:{token_uuid}
// ARGV[1]: expected business_id (decimal string, e.g. "42")
//
// Return shape — Redis array:
//   {0}          token not found, expired, or already consumed → Unauthorized
//   {1}          token owned by a different business; NOT consumed → Forbidden
//   {2, payload} success; token consumed; payload = "{biz_id}:{user_id}"
const QR_CONSUME_LUA: &str = r#"
local val = redis.call('GET', KEYS[1])
if not val then return {0} end
local stored_biz = string.match(val, '^(%d+):')
if stored_biz ~= ARGV[1] then return {1} end
redis.call('DEL', KEYS[1])
return {2, val}
"#;

const QR_TOKEN_TTL_SECS: u64 = 300; // 5 minutes
const BALANCE_RATE_LIMIT: i64 = 10; // per user per minute
const STAMP_RATE_LIMIT:   i64 = 30; // per business per minute

// ── Balance ───────────────────────────────────────────────────────────────────

pub async fn get_balance(
    state:       &AppState,
    user_id:     UserId,
    business_id: i32,
    ip:          Option<std::net::IpAddr>,
) -> AppResult<LoyaltyBalance> {
    rate_check_balance(state, user_id).await?;

    let cfg = repository::get_config(&state.db, business_id)
        .await?
        .ok_or_else(|| AppError::NotFound)?;

    let email_verified = crate::domain::auth::repository::get_verified(&state.db, user_id)
        .await
        .unwrap_or(true); // fail open — don't block balance reads on a DB error

    let raw = repository::get_balance(&state.db, user_id, business_id).await?;

    let steeps_per_reward = cfg.steeps_per_reward as i64;
    let credits_spent     = raw.rewards_redeemed * steeps_per_reward;
    let current_balance   = raw.steeps_earned.saturating_sub(credits_spent);
    let steeps_until_reward = (steeps_per_reward - (current_balance % steeps_per_reward))
        % steeps_per_reward;
    let reward_available = current_balance >= steeps_per_reward;

    audit::write(
        &state.db,
        Some(user_id.into()),
        Some(business_id),
        "loyalty.balance_read",
        serde_json::json!({
            "steeps_earned":    raw.steeps_earned,
            "rewards_redeemed": raw.rewards_redeemed,
        }),
        ip,
    ).await;

    Ok(LoyaltyBalance {
        steeps_earned:      raw.steeps_earned,
        rewards_redeemed:   raw.rewards_redeemed,
        current_balance,
        steeps_per_reward:  cfg.steeps_per_reward,
        reward_description: cfg.reward_description,
        steeps_until_reward,
        reward_available,
        email_verified,
    })
}

// ── History ───────────────────────────────────────────────────────────────────

pub async fn get_history(
    state:       &AppState,
    user_id:     UserId,
    business_id: i32,
    limit:       i64,
    offset:      i64,
) -> AppResult<Vec<LoyaltyEventRow>> {
    let limit  = limit.clamp(1, 50);
    let offset = offset.max(0);
    repository::get_history(&state.db, user_id, business_id, limit, offset).await
}

// ── QR token generation ───────────────────────────────────────────────────────

/// Generates a single-use QR stamp token for the customer.
/// The token encodes both user_id and business_id — cross-business use is
/// detected and rejected at redemption time.
pub async fn issue_qr_token(
    state:       &AppState,
    user_id:     UserId,
    business_id: i32,
) -> AppResult<QrTokenResponse> {
    repository::get_config(&state.db, business_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Gate walk-in stamps on email verification. In-app payments credit steeps
    // regardless — a cleared Stripe charge is a stronger signal than an email click.
    let verified = crate::domain::auth::repository::get_verified(&state.db, user_id)
        .await
        .unwrap_or(false);
    if !verified {
        return Err(AppError::Unprocessable(
            "verify your email to start earning steeps".into()
        ));
    }

    let redis_pool = state.redis.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "Redis required for QR token generation — set REDIS_URL"
        )))?;

    let token = Uuid::new_v4().to_string();
    let key   = format!("fraise:stamp:{token}");
    // Value encodes both IDs so the stamp endpoint can validate without a DB lookup.
    let value = format!("{business_id}:{}", i32::from(user_id));

    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis pool: {e}")))?;

    let result: redis::Value = redis::cmd("SET")
        .arg(&key)
        .arg(&value)
        .arg("EX")
        .arg(QR_TOKEN_TTL_SECS)
        .arg("NX")
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis SET stamp token: {e}")))?;

    if matches!(result, redis::Value::Nil) {
        // UUID collision — astronomically unlikely, but fail loudly rather than silently.
        return Err(AppError::Internal(anyhow::anyhow!("QR token UUID collision — retry")));
    }

    let expires_at = Utc::now() + Duration::seconds(QR_TOKEN_TTL_SECS as i64);

    audit::write(
        &state.db,
        Some(user_id.into()),
        Some(business_id),
        "loyalty.qr_token_issued",
        serde_json::json!({ "token_prefix": &token[..8] }), // partial token only
        None,
    ).await;

    Ok(QrTokenResponse { token, expires_at })
}

// ── Stamp via QR (JSON path — iOS app scanner) ────────────────────────────────

/// Records a steep from a staff member who scanned the customer's QR via the app.
/// RequireStaff ensures the staff JWT's business_id matches the token's.
pub async fn stamp_via_qr(
    state:          &AppState,
    staff_user_id:  UserId,
    staff_business: i32,
    qr_token:       &str,
    ip:             Option<std::net::IpAddr>,
) -> AppResult<StampResult> {
    rate_check_stamp(state, staff_business).await?;

    let (customer_id, token_business) = match consume_qr_token_for_business(state, qr_token, staff_business).await {
        Err(AppError::Forbidden) => {
            // Token exists but belongs to a different business and was NOT consumed.
            audit::write(
                &state.db,
                Some(staff_user_id.into()),
                Some(staff_business),
                "loyalty.cross_business_stamp_rejected",
                serde_json::json!({ "staff_business_id": staff_business }),
                ip,
            ).await;
            return Err(AppError::Forbidden);
        }
        other => other?,
    };

    record_steep(state, customer_id, token_business, "qr_stamp", qr_token, Some(staff_user_id), ip).await
}

// ── Stamp via HTML page (fallback — camera scan without app) ─────────────────

/// Records a steep via the HTML /stamp page. Security model: the QR token
/// itself encodes the business_id, so cross-business stamping is structurally
/// impossible. No staff JWT required — the token IS the proof of intent.
pub async fn stamp_via_html(
    state:       &AppState,
    qr_token:    &str,
    claimed_bid: i32, // ?b= query param — verified against the token's stored business_id
    ip:          Option<std::net::IpAddr>,
) -> AppResult<StampResult> {
    rate_check_stamp(state, claimed_bid).await?;
    let (customer_id, token_business) = consume_qr_token_for_business(state, qr_token, claimed_bid).await?;
    record_steep(state, customer_id, token_business, "qr_stamp", qr_token, None, ip).await
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Atomically validates business ownership and consumes a QR stamp token.
///
/// Uses QR_CONSUME_LUA to GET, check, and DEL in one Redis round-trip.
/// Returns:
///   Ok((customer_id, business_id)) — token valid for this business, now consumed
///   Err(Forbidden)                 — token exists but owned by a different business;
///                                    the token is preserved so the customer can retry
///   Err(Unauthorized)              — token not found, expired, or already consumed
async fn consume_qr_token_for_business(
    state:        &AppState,
    token:        &str,
    expected_biz: i32,
) -> AppResult<(UserId, i32)> {
    let redis_pool = state.redis.as_ref().ok_or(AppError::Unauthorized)?;
    let key = format!("fraise:stamp:{token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis pool: {e}")))?;

    let result: redis::Value = redis::Script::new(QR_CONSUME_LUA)
        .key(&key)
        .arg(expected_biz.to_string())
        .invoke_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis Lua stamp consume: {e}")))?;

    // Match the return codes defined in QR_CONSUME_LUA.
    let parts = match result {
        redis::Value::Bulk(v) => v,
        other => return Err(AppError::Internal(
            anyhow::anyhow!("unexpected Redis type from Lua consume script: {other:?}")
        )),
    };

    match parts.as_slice() {
        [redis::Value::Int(0)] => Err(AppError::Unauthorized),
        [redis::Value::Int(1)] => Err(AppError::Forbidden),
        [redis::Value::Int(2), redis::Value::Data(raw)] => parse_token_value(raw),
        _ => Err(AppError::Internal(anyhow::anyhow!("unexpected Lua response shape: {parts:?}"))),
    }
}

fn parse_token_value(raw: &[u8]) -> AppResult<(UserId, i32)> {
    // Value format stored in Redis: "{business_id}:{user_id}"
    let s = std::str::from_utf8(raw)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("non-UTF8 stamp token value")))?;
    let mut parts = s.splitn(2, ':');
    let biz = parts.next()
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("malformed stamp token: missing business_id")))?;
    let uid = parts.next()
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("malformed stamp token: missing user_id")))?;
    Ok((UserId::from(uid), biz))
}

async fn record_steep(
    state:       &AppState,
    customer_id: UserId,
    business_id: i32,
    source:      &str,
    idem_key:    &str,
    actor_id:    Option<UserId>,
    ip:          Option<std::net::IpAddr>,
) -> AppResult<StampResult> {
    repository::insert_event(
        &state.db,
        customer_id,
        business_id,
        "steep_earned",
        source,
        idem_key,
        serde_json::json!({}),
    ).await
    .map_err(|e| match e {
        // idempotency_key UNIQUE violation — this token was already redeemed.
        // This path should be unreachable because GETDEL is atomic, but if it
        // somehow fires (e.g., two concurrent requests with the same token before
        // Redis GETDEL could run), we treat it as a duplicate, not an error.
        AppError::Db(sqlx::Error::Database(ref db)) if db.is_unique_violation() => {
            AppError::Conflict("steep already recorded for this token".into())
        }
        other => other,
    })?;

    let cfg = repository::get_config(&state.db, business_id)
        .await?
        .unwrap_or_else(|| super::types::LoyaltyConfig {
            steeps_per_reward:  10,
            reward_description: "one free drink".into(),
        });

    let raw = repository::get_balance(&state.db, customer_id, business_id).await?;
    let steeps_per_reward = cfg.steeps_per_reward as i64;
    let credits_spent  = raw.rewards_redeemed * steeps_per_reward;
    let current_balance = raw.steeps_earned.saturating_sub(credits_spent);
    let reward_available = current_balance >= steeps_per_reward;

    let customer_name = repository::get_customer_name(&state.db, customer_id).await?;

    audit::write(
        &state.db,
        actor_id.map(|id| id.into()),
        Some(business_id),
        "loyalty.steep_earned",
        serde_json::json!({
            "customer_id":    i32::from(customer_id),
            "new_balance":    current_balance,
            "reward_available": reward_available,
            "source":         source,
        }),
        ip,
    ).await;

    Ok(StampResult {
        business_id,
        customer_name,
        new_balance:        current_balance,
        reward_available,
        reward_description: cfg.reward_description,
    })
}

// ── NFC cup stickers ──────────────────────────────────────────────────────────

const NFC_PREFIX:  &str = "fraise:nfc-active:";
const NFC_TTL:     u64  = 7_200; // 2 hours — window between preparation and collection

/// Called by staff after scanning the companion QR on a sticker.
/// Registers the sticker to the staff's business (or validates it's already theirs)
/// and sets a 2-hour activation window in Redis.
pub async fn activate_nfc_sticker(
    state:          &AppState,
    staff_uid:      UserId,
    staff_business: i32,
    sticker_uuid:   &str,
) -> AppResult<()> {
    repository::upsert_nfc_sticker(&state.db, sticker_uuid, staff_business).await?;

    let redis_pool = state.redis.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Redis required for NFC activation")))?;

    let key = format!("{NFC_PREFIX}{sticker_uuid}");
    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    // SET overwrites any previous activation — staff can re-activate if needed.
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(staff_business.to_string())
        .arg("EX").arg(NFC_TTL)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis SET: {e}")))?;

    audit::write(
        &state.db,
        Some(staff_uid.into()),
        Some(staff_business),
        "loyalty.nfc_sticker_activated",
        serde_json::json!({ "sticker_uuid": sticker_uuid }),
        None,
    ).await;

    Ok(())
}

/// Called by the iOS app when the customer taps an NFC sticker.
/// Atomically consumes the activation window and credits a loyalty steep.
pub async fn redeem_nfc_sticker(
    state:        &AppState,
    user_id:      UserId,
    sticker_uuid: &str,
    ip:           Option<std::net::IpAddr>,
) -> AppResult<StampResult> {
    // Email verification required for NFC redemption (walk-in path, no payment).
    let verified = crate::domain::auth::repository::get_verified(&state.db, user_id)
        .await
        .unwrap_or(false);
    if !verified {
        return Err(AppError::Unprocessable(
            "verify your email to earn steeps".into()
        ));
    }

    let redis_pool = state.redis.as_ref().ok_or(AppError::Unauthorized)?;
    let key = format!("{NFC_PREFIX}{sticker_uuid}");
    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    // GETDEL: atomically read and consume the activation window.
    let value: Option<String> = redis::cmd("GETDEL")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let business_id = value
        .ok_or(AppError::Unauthorized)? // not activated or window expired
        .parse::<i32>()
        .map_err(|_| AppError::Internal(anyhow::anyhow!("malformed NFC activation value")))?;

    // Idempotency key: sticker_uuid — unique per activation window (re-activation generates a new window).
    let result = record_steep(
        state, user_id, business_id, "nfc_tap", sticker_uuid, None, ip,
    ).await?;

    let _ = repository::increment_nfc_taps(&state.db, sticker_uuid).await;

    audit::write(
        &state.db,
        Some(user_id.into()),
        Some(business_id),
        "loyalty.nfc_sticker_redeemed",
        serde_json::json!({ "sticker_uuid": sticker_uuid }),
        ip,
    ).await;

    Ok(result)
}

// ── Webhook path (called by venue_drinks on payment_intent.succeeded) ─────────

/// Records a steep triggered by a confirmed in-app payment.
/// Idempotency key is the Stripe payment_intent_id — safe to retry.
pub async fn record_steep_from_webhook(
    state:           &AppState,
    user_id:         UserId,
    business_id:     i32,
    idempotency_key: &str,
) -> AppResult<()> {
    repository::insert_event(
        &state.db,
        user_id,
        business_id,
        "steep_earned",
        "stripe_webhook",
        idempotency_key,
        serde_json::json!({ "stripe_payment_intent_id": idempotency_key }),
    ).await
    .map_err(|e| match e {
        AppError::Db(sqlx::Error::Database(ref db)) if db.is_unique_violation() => {
            // Already recorded for this payment — idempotent success.
            AppError::Conflict("loyalty event already recorded".into())
        }
        other => other,
    })?;

    audit::write(
        &state.db,
        Some(user_id.into()),
        Some(business_id),
        "loyalty.steep_earned",
        serde_json::json!({
            "customer_id": i32::from(user_id),
            "source":      "stripe_webhook",
            "idempotency_key": idempotency_key,
        }),
        None,
    ).await;

    Ok(())
}

// ── Rate limiting ─────────────────────────────────────────────────────────────

/// 10 balance reads per user per minute.
async fn rate_check_balance(state: &AppState, user_id: UserId) -> AppResult<()> {
    rate_check(state, &format!("fraise:rate:loyalty-bal:{}", i32::from(user_id)), BALANCE_RATE_LIMIT).await
}

/// 30 stamp attempts per business per minute.
async fn rate_check_stamp(state: &AppState, business_id: i32) -> AppResult<()> {
    rate_check(state, &format!("fraise:rate:loyalty-stamp:{business_id}"), STAMP_RATE_LIMIT).await
}

/// Fixed-window counter using Redis INCR + EXPIRE.
/// If Redis is absent, rate limiting is skipped (single-instance deployments
/// already have the global IP limiter; loyalty-specific limits are best-effort).
async fn rate_check(state: &AppState, key: &str, limit: i64) -> AppResult<()> {
    let Some(redis_pool) = state.redis.as_ref() else { return Ok(()) };

    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis pool: {e}")))?;

    // INCR returns the new value; if it was 0 before, set a 60-second window.
    let count: i64 = redis::cmd("INCR")
        .arg(key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR rate: {e}")))?;

    if count == 1 {
        // First request in this window — set expiry.
        let _: () = redis::cmd("EXPIRE")
            .arg(key)
            .arg(60u64)
            .query_async(&mut *conn)
            .await
            .unwrap_or(());
    }

    if count > limit {
        Err(AppError::Unprocessable("rate limit exceeded — try again shortly".into()))
    } else {
        Ok(())
    }
}
