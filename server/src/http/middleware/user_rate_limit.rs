//! Per-user, per-endpoint rate limiter (Hardening §6 / Grade A item 3).
//!
//! Sits behind the JWT extractor — keys on `(user_id, endpoint, window)`
//! and reads the limit from `platform_configuration` so operators can
//! retune without a redeploy. The four limits seeded by migration
//! `009_rate_limits.sql` are wired into the four routes the design calls
//! out (attestations, background-checks, identity, dorotka).
//!
//! Counter shape mirrors the global IP limiter: Redis `INCR` with a TTL
//! on first hit. When Redis is absent the limiter fails open and logs;
//! per-user limits are an enforcement layer, not the only one — the
//! pre-auth IP bucket and HMAC middleware already gate abuse.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use deadpool_redis::{redis, Pool as RedisPool};
use sqlx::PgPool;

use box_fraise_domain::domain::platform_configuration::service as config_svc;

/// Window granularity for a per-user limit.
#[derive(Debug, Clone, Copy)]
pub enum RateLimitWindow {
    /// Resets at the top of every UTC hour.
    Hourly,
    /// Resets at UTC midnight.
    Daily,
}

impl RateLimitWindow {
    /// String key embedded in the Redis key — same value within a window.
    pub fn window_key(self) -> String {
        match self {
            Self::Hourly => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (now / 3600 * 3600).to_string()
            }
            Self::Daily => chrono::Utc::now().format("%Y-%m-%d").to_string(),
        }
    }

    /// Redis TTL — window length plus 60s buffer to ride out clock skew.
    pub fn ttl_secs(self) -> u64 {
        match self {
            Self::Hourly => 3_660,
            Self::Daily  => 86_460,
        }
    }
}

/// Limit value to apply when `platform_configuration` lookup fails.
/// Chosen permissive — the goal is "never deny a real user because Redis
/// or Postgres burped"; the global IP bucket still caps abuse.
const SAFE_DEFAULT_LIMIT: i64 = 100;

pub struct UserRateLimiter {
    redis: Option<RedisPool>,
    pool:  PgPool,
}

impl UserRateLimiter {
    pub fn new(redis: Option<RedisPool>, pool: PgPool) -> Arc<Self> {
        Arc::new(Self { redis, pool })
    }

    /// Check-and-increment the per-user counter.
    ///
    /// Returns `Ok(())` when the request is under the limit, or
    /// `Err(remaining_ttl_secs)` when over — the caller maps that into
    /// HTTP 429 with `Retry-After`.
    ///
    /// Failure modes (Redis unavailable, Postgres lookup error) are
    /// logged and treated as "allow". The pre-auth IP limiter and the
    /// HMAC middleware are the always-on backstops; this layer's job is
    /// abuse-cost capping per authenticated identity, not authentication.
    pub async fn check(
        &self,
        user_id:      i32,
        endpoint_key: &str,
        config_key:   &str,
        window:       RateLimitWindow,
    ) -> Result<(), u64> {
        let Some(redis) = self.redis.as_ref() else {
            // No Redis configured (single-instance dev) — skip silently.
            return Ok(());
        };

        let limit = match config_svc::get_configuration(&self.pool, config_key).await {
            Ok(row) => row.value.parse::<i64>().unwrap_or(SAFE_DEFAULT_LIMIT),
            Err(e) => {
                tracing::warn!(error = %e, key = config_key, "rate limit config lookup failed — using safe default");
                SAFE_DEFAULT_LIMIT
            }
        };

        let mut conn = match redis.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "user rate limit Redis pool error — allowing request");
                return Ok(());
            }
        };

        let key = format!("rate:{}:{}:{}", user_id, endpoint_key, window.window_key());

        let count: i64 = match redis::cmd("INCR").arg(&key).query_async(&mut *conn).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = %e, "user rate limit INCR failed — allowing request");
                return Ok(());
            }
        };

        if count == 1 {
            let _: () = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(window.ttl_secs())
                .query_async(&mut *conn)
                .await
                .unwrap_or(());
        }

        if count > limit {
            // Best-effort: ask Redis how long until the key expires so the
            // client gets an accurate Retry-After. If the TTL read fails or
            // returns a negative value, fall back to the window's nominal TTL.
            let ttl: i64 = redis::cmd("TTL")
                .arg(&key)
                .query_async(&mut *conn)
                .await
                .unwrap_or(-1);
            let retry = if ttl > 0 { ttl as u64 } else { window.ttl_secs() };
            return Err(retry);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn hourly_window_key_stable_within_hour() {
        let a = RateLimitWindow::Hourly.window_key();
        thread::sleep(Duration::from_millis(20));
        let b = RateLimitWindow::Hourly.window_key();
        assert_eq!(a, b, "hourly key must be identical across rapid successive calls");
    }

    #[test]
    fn hourly_and_daily_keys_differ_in_format() {
        let h = RateLimitWindow::Hourly.window_key();
        let d = RateLimitWindow::Daily.window_key();
        // Hourly is a unix timestamp (digits only); daily is YYYY-MM-DD.
        assert!(h.chars().all(|c| c.is_ascii_digit()),
            "hourly key must be digits only, got {h:?}");
        assert!(d.contains('-'),
            "daily key must be YYYY-MM-DD, got {d:?}");
    }

    #[test]
    fn ttl_includes_buffer() {
        assert!(RateLimitWindow::Hourly.ttl_secs() > 3600,
            "hourly TTL must include buffer beyond the window length");
        assert!(RateLimitWindow::Daily.ttl_secs() > 86_400,
            "daily TTL must include buffer beyond the window length");
    }
}
