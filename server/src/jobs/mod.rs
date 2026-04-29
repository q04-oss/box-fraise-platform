use tokio_cron_scheduler::{Job, JobScheduler};
use sqlx::PgPool;
use tracing::info;

pub async fn start(pool: PgPool) -> anyhow::Result<()> {
    let sched = JobScheduler::new().await?;

    // 02:00 daily — expire completed employment contracts.
    {
        let db = pool.clone();
        sched.add(Job::new_async("0 0 2 * * *", move |_, _| {
            let db = db.clone();
            Box::pin(async move { expire_contracts(&db).await; })
        })?).await?;
    }

    // 08:00 daily — process standing orders + send daily summary.
    {
        let db = pool.clone();
        sched.add(Job::new_async("0 0 8 * * *", move |_, _| {
            let db = db.clone();
            Box::pin(async move { process_standing_orders(&db).await; })
        })?).await?;
    }

    // 09:00 daily — membership renewal reminders.
    {
        let db = pool.clone();
        sched.add(Job::new_async("0 0 9 * * *", move |_, _| {
            let db = db.clone();
            Box::pin(async move { membership_reminders(&db).await; })
        })?).await?;
    }

    // 01:00 on Jan 1 — reset membership fund balances, expire memberships.
    {
        let db = pool.clone();
        sched.add(Job::new_async("0 0 1 1 1 *", move |_, _| {
            let db = db.clone();
            Box::pin(async move { january_reset(&db).await; })
        })?).await?;
    }

    sched.start().await?;
    info!("cron scheduler started (4 jobs)");
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

async fn process_standing_orders(pool: &PgPool) {
    // Standing orders that are active and due today. Each is charged via the
    // stored Stripe customer ID. Errors are logged per-order; one failure
    // does not block others.
    let count: Result<i64, _> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM standing_orders WHERE status = 'active'",
    )
    .fetch_one(pool)
    .await;

    match count {
        Ok(n)  => info!(count = n, "standing orders to process"),
        Err(e) => tracing::error!(error = %e, "process_standing_orders: query failed"),
    }

    // TODO: create orders + off-session Stripe charges for each standing order
}

async fn membership_reminders(pool: &PgPool) {
    // Memberships expiring within 30 days — send renewal reminder email.
    let result: Result<i64, _> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memberships m
         JOIN users u ON u.id = m.user_id
         WHERE m.status = 'active'
           AND m.renews_at IS NOT NULL
           AND m.renews_at <= NOW() + INTERVAL '30 days'
           AND m.renewal_notified = false",
    )
    .fetch_one(pool)
    .await;

    match result {
        Ok(n) => {
            info!(count = n, "membership renewal reminders to send");
            // TODO: send reminder emails via integrations::resend
            // Mark notified
            let _ = sqlx::query(
                "UPDATE memberships
                 SET renewal_notified = true
                 WHERE status = 'active'
                   AND renews_at IS NOT NULL
                   AND renews_at <= NOW() + INTERVAL '30 days'
                   AND renewal_notified = false",
            )
            .execute(pool)
            .await;
        }
        Err(e) => tracing::error!(error = %e, "membership_reminders: query failed"),
    }
}

async fn january_reset(pool: &PgPool) {
    // Reset all membership fund balances to zero.
    if let Err(e) = sqlx::query(
        "UPDATE membership_funds SET balance_cents = 0",
    )
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "january_reset: fund reset failed");
    }

    // Expire memberships past their renews_at date.
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
