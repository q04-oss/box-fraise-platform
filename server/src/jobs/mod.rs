use secrecy::ExposeSecret;
use sqlx::PgPool;

use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

use crate::app::AppState;
use crate::integrations::resend;

pub async fn start(state: AppState) -> anyhow::Result<()> {
    let sched = JobScheduler::new().await?;

    // 02:00 daily — expire completed employment contracts.
    {
        let db = state.db.clone();
        sched.add(Job::new_async("0 0 2 * * *", move |_, _| {
            let db = db.clone();
            Box::pin(async move { expire_contracts(&db).await; })
        })?).await?;
    }

    // 09:00 daily — membership renewal reminders.
    {
        let s = state.clone();
        sched.add(Job::new_async("0 0 9 * * *", move |_, _| {
            let s = s.clone();
            Box::pin(async move { membership_reminders(&s).await; })
        })?).await?;
    }

    // 01:00 on Jan 1 — reset membership fund balances, expire memberships.
    {
        let db = state.db.clone();
        sched.add(Job::new_async("0 0 1 1 1 *", move |_, _| {
            let db = db.clone();
            Box::pin(async move { january_reset(&db).await; })
        })?).await?;
    }

    sched.start().await?;
    info!("cron scheduler started (3 jobs)");
    Ok(())
}

// ── Job implementations ───────────────────────────────────────────────────────

async fn expire_contracts(pool: &PgPool) {
    let result = sqlx::query(
        "UPDATE employment_contracts
         SET status = 'completed'
         WHERE status = 'active'
           AND end_date IS NOT NULL
           AND end_date < NOW()",
    )
    .execute(pool)
    .await;

    match result {
        Ok(r)  => info!(rows = r.rows_affected(), "expired employment contracts"),
        Err(e) => tracing::error!(error = %e, "expire_contracts job failed"),
    }
}

// ── Membership reminders ──────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct MembershipReminder {
    user_id:   i32,
    email:     Option<String>,
    tier:      String,
    renews_at: chrono::NaiveDateTime,
}

async fn membership_reminders(state: &AppState) {
    let rows: Vec<MembershipReminder> = match sqlx::query_as(
        "SELECT m.user_id, u.email, m.tier, m.renews_at
         FROM memberships m
         JOIN users u ON u.id = m.user_id
         WHERE m.status = 'active'
           AND m.renews_at IS NOT NULL
           AND m.renews_at <= NOW() + INTERVAL '30 days'
           AND m.renewal_notified = false",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(r)  => r,
        Err(e) => {
            tracing::error!(error = %e, "membership_reminders: query failed");
            return;
        }
    };

    info!(count = rows.len(), "sending membership renewal reminders");

    for row in &rows {
        if let (Some(ref email), Some(ref key)) = (&row.email, &state.cfg.resend_api_key) {
            let http      = state.http.clone();
            let key       = key.expose_secret().to_owned();
            let email     = email.clone();
            let tier      = row.tier.clone();
            let renews_at = row.renews_at;
            tokio::spawn(async move {
                send_renewal_reminder(&http, &key, &email, &tier, renews_at).await;
            });
        }
    }

    // Mark all reminded in one shot.
    if !rows.is_empty() {
        if let Err(e) = sqlx::query(
            "UPDATE memberships
             SET renewal_notified = true
             WHERE status = 'active'
               AND renews_at IS NOT NULL
               AND renews_at <= NOW() + INTERVAL '30 days'
               AND renewal_notified = false",
        )
        .execute(&state.db)
        .await
        {
            tracing::error!(error = %e, "memberships renewal_notified UPDATE failed — reminders will re-fire on next run");
        }
    }
}

async fn send_renewal_reminder(
    http:      &reqwest::Client,
    key:       &str,
    email:     &str,
    tier:      &str,
    renews_at: chrono::NaiveDateTime,
) {
    let days_left = (renews_at - chrono::Utc::now().naive_utc()).num_days().max(0);
    let subject   = format!("Your {tier} membership renews in {days_left} days — Maison Fraise");
    let html      = resend::renewal_reminder_html(tier, renews_at, days_left);
    if let Err(e) = resend::send(http, key, email, &subject, &html).await {
        tracing::error!(tier, error = %e, "membership renewal reminder email delivery failed");
    }
}

// ── January reset ─────────────────────────────────────────────────────────────

async fn january_reset(pool: &PgPool) {
    if let Err(e) = sqlx::query(
        "UPDATE memberships
         SET status = 'expired'
         WHERE status = 'active'
           AND renews_at IS NOT NULL
           AND renews_at < NOW()",
    )
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "january_reset: membership expiry failed");
    }

    info!("january reset complete");
}
