//! Aggregate analytics queries — Hardening §5.
//!
//! Every function in this module is a pure SELECT. No writes, no triggers,
//! no side effects. The HTTP layer above gates access to `platform_admin`;
//! queries themselves do not check authorization.

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;

// ── Verification funnel ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct FunnelStage {
    pub stage:              String,
    pub user_count:         i64,
    pub avg_days_at_stage:  Option<f64>,
}

/// Snapshot of users grouped by current `verification_status`.
///
/// `users.is_banned` is the schema's flag for banned users (the section spec
/// referred to a non-existent `banned_at` column).
pub async fn verification_funnel(pool: &PgPool) -> Result<Vec<FunnelStage>, sqlx::Error> {
    sqlx::query_as::<_, FunnelStage>(
        "SELECT \
            verification_status AS stage, \
            COUNT(*)::bigint     AS user_count, \
            AVG(EXTRACT(EPOCH FROM (now() - created_at)) / 86400.0)::float8 AS avg_days_at_stage \
         FROM users \
         WHERE deleted_at IS NULL AND NOT is_banned \
         GROUP BY verification_status \
         ORDER BY \
            CASE verification_status \
                WHEN 'registered'          THEN 1 \
                WHEN 'identity_confirmed'  THEN 2 \
                WHEN 'presence_confirmed'  THEN 3 \
                WHEN 'attested'            THEN 4 \
                ELSE 5 \
            END"
    )
    .fetch_all(pool)
    .await
}

// ── Daily counts (shared shape) ─────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct DailyCount {
    pub date:  NaiveDate,
    pub count: i64,
}

/// Soultokens issued per day (last 30 days), sourced from `verification_events`.
pub async fn daily_attestations(pool: &PgPool) -> Result<Vec<DailyCount>, sqlx::Error> {
    sqlx::query_as::<_, DailyCount>(
        "SELECT \
            DATE(created_at)::date AS date, \
            COUNT(*)::bigint        AS count \
         FROM verification_events \
         WHERE event_type = 'soultoken_issued' \
           AND created_at > now() - INTERVAL '30 days' \
         GROUP BY DATE(created_at) \
         ORDER BY date DESC"
    )
    .fetch_all(pool)
    .await
}

/// Presence events per day (last 14 days).
pub async fn daily_presence_events(pool: &PgPool) -> Result<Vec<DailyCount>, sqlx::Error> {
    sqlx::query_as::<_, DailyCount>(
        "SELECT \
            calendar_date AS date, \
            COUNT(*)::bigint AS count \
         FROM presence_events \
         WHERE calendar_date > CURRENT_DATE - INTERVAL '14 days' \
         GROUP BY calendar_date \
         ORDER BY calendar_date DESC"
    )
    .fetch_all(pool)
    .await
}

// ── Time to attest ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct TimeToAttest {
    pub avg_days:    Option<f64>,
    pub median_days: Option<f64>,
    pub p90_days:    Option<f64>,
}

/// Distribution of days from `identity_confirmed` to `soultoken_issued`.
pub async fn time_to_attest(pool: &PgPool) -> Result<TimeToAttest, sqlx::Error> {
    sqlx::query_as::<_, TimeToAttest>(
        "SELECT \
            AVG(EXTRACT(EPOCH FROM (attested.created_at - registered.created_at)) / 86400.0)::float8 AS avg_days, \
            PERCENTILE_CONT(0.5) WITHIN GROUP ( \
                ORDER BY EXTRACT(EPOCH FROM (attested.created_at - registered.created_at)) / 86400.0 \
            )::float8 AS median_days, \
            PERCENTILE_CONT(0.9) WITHIN GROUP ( \
                ORDER BY EXTRACT(EPOCH FROM (attested.created_at - registered.created_at)) / 86400.0 \
            )::float8 AS p90_days \
         FROM verification_events attested \
         JOIN verification_events registered \
           ON attested.user_id = registered.user_id \
         WHERE attested.event_type   = 'soultoken_issued' \
           AND registered.event_type = 'identity_confirmed'"
    )
    .fetch_one(pool)
    .await
}

// ── Business stats ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct BusinessStats {
    pub total_active: i64,
    pub with_beacon:  i64,
    pub suspended:    i64,
}

pub async fn business_stats(pool: &PgPool) -> Result<BusinessStats, sqlx::Error> {
    sqlx::query_as::<_, BusinessStats>(
        "SELECT \
            COUNT(*) FILTER (WHERE is_active)::bigint AS total_active, \
            COUNT(*) FILTER ( \
                WHERE is_active AND EXISTS ( \
                    SELECT 1 FROM beacons \
                    WHERE beacons.location_id = businesses.location_id \
                ) \
            )::bigint AS with_beacon, \
            COUNT(*) FILTER (WHERE beacon_suspended)::bigint AS suspended \
         FROM businesses"
    )
    .fetch_one(pool)
    .await
}

// ── Soultoken stats ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct SoultokenStats {
    pub total_issued:     i64,
    pub currently_active: i64,
    pub revoked:          i64,
    pub expired:          i64,
    pub renewal_rate_30d: Option<f64>,
}

pub async fn soultoken_stats(pool: &PgPool) -> Result<SoultokenStats, sqlx::Error> {
    sqlx::query_as::<_, SoultokenStats>(
        "SELECT \
            COUNT(*)::bigint AS total_issued, \
            COUNT(*) FILTER (WHERE revoked_at IS NULL AND expires_at > now())::bigint AS currently_active, \
            COUNT(*) FILTER (WHERE revoked_at IS NOT NULL)::bigint                    AS revoked, \
            COUNT(*) FILTER (WHERE revoked_at IS NULL AND expires_at <= now())::bigint AS expired, \
            ( \
                SELECT (COUNT(*)::float8 / NULLIF(COUNT(DISTINCT user_id), 0))::float8 \
                FROM soultoken_renewals \
                WHERE renewed_at > now() - INTERVAL '30 days' \
            ) AS renewal_rate_30d \
         FROM soultokens \
         WHERE token_type = 'user'"
    )
    .fetch_one(pool)
    .await
}

// ── Background-check pass rate ──────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct BackgroundCheckStats {
    pub check_type: String,
    pub total:      i64,
    pub passed:     i64,
    pub failed:     i64,
    pub pass_rate:  Option<f64>,
}

pub async fn background_check_stats(pool: &PgPool) -> Result<Vec<BackgroundCheckStats>, sqlx::Error> {
    sqlx::query_as::<_, BackgroundCheckStats>(
        "SELECT \
            check_type, \
            COUNT(*)::bigint                                                      AS total, \
            COUNT(*) FILTER (WHERE status = 'passed')::bigint                     AS passed, \
            COUNT(*) FILTER (WHERE status = 'failed')::bigint                     AS failed, \
            (COUNT(*) FILTER (WHERE status = 'passed')::float8 / NULLIF(COUNT(*), 0))::float8 AS pass_rate \
         FROM background_checks \
         WHERE checked_at IS NOT NULL \
         GROUP BY check_type \
         ORDER BY check_type"
    )
    .fetch_all(pool)
    .await
}

// ── Conversion drop-off ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DropOff {
    pub from_stage:      String,
    pub to_stage:        String,
    pub from_count:      i64,
    pub to_count:        i64,
    pub conversion_rate: Option<f64>,
}

/// Adjacent-stage conversion rates derived from `verification_funnel`.
///
/// `verification_status` is a *snapshot* (the user's current stage), so the
/// funnel `Vec` shows users currently at each stage. To get meaningful
/// conversion numbers we compute *cumulative* counts: a user currently
/// `attested` once also passed through `presence_confirmed`, `identity_confirmed`,
/// and `registered`. So `from_count` for each pair is the sum of the "from"
/// stage AND every stage past it; `to_count` is the sum of the "to" stage
/// onward.
pub async fn conversion_dropoff(pool: &PgPool) -> Result<Vec<DropOff>, sqlx::Error> {
    let funnel = verification_funnel(pool).await?;
    let order = ["registered", "identity_confirmed", "presence_confirmed", "attested"];

    let count_of = |stage: &str| -> i64 {
        funnel.iter().find(|s| s.stage == stage).map(|s| s.user_count).unwrap_or(0)
    };
    let cumulative_from = |stage_idx: usize| -> i64 {
        order[stage_idx..].iter().map(|s| count_of(s)).sum()
    };

    let mut out = Vec::with_capacity(order.len() - 1);
    for i in 0..order.len() - 1 {
        let from_count = cumulative_from(i);
        let to_count   = cumulative_from(i + 1);
        let conversion_rate = if from_count > 0 {
            Some(to_count as f64 / from_count as f64)
        } else {
            None
        };
        out.push(DropOff {
            from_stage:      order[i].to_string(),
            to_stage:        order[i + 1].to_string(),
            from_count,
            to_count,
            conversion_rate,
        });
    }
    Ok(out)
}
