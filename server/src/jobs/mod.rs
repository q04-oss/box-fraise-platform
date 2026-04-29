use secrecy::ExposeSecret;
use sqlx::PgPool;

use crate::types::StripeCustomerId;
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

    // 08:00 daily — process standing orders.
    {
        let s = state.clone();
        sched.add(Job::new_async("0 0 8 * * *", move |_, _| {
            let s = s.clone();
            Box::pin(async move { process_standing_orders(&s).await; })
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

// ── Standing orders ───────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct StandingOrder {
    id:                     i32,
    user_id:                i32,
    variety_id:             i32,
    location_id:            i32,
    quantity:               i32,
    total_cents:            i64,
    frequency:              String, // "daily" | "weekly" | "monthly"
    stripe_payment_method_id: String,
    stripe_customer_id:     StripeCustomerId,
    user_email:             Option<String>,
    variety_name:           String,
}

async fn process_standing_orders(state: &AppState) {
    // Fetch all active standing orders due today where the user has a saved
    // payment method. A LIMIT guards against runaway batch size.
    let orders: Vec<StandingOrder> = match sqlx::query_as(
        "SELECT
             so.id, so.user_id, so.variety_id, so.location_id,
             so.quantity, so.total_cents, so.frequency,
             so.stripe_payment_method_id,
             u.stripe_customer_id,
             u.email      AS user_email,
             v.name       AS variety_name
         FROM standing_orders so
         JOIN users u ON u.id = so.user_id
         JOIN catalog_varieties v ON v.id = so.variety_id
         WHERE so.status = 'active'
           AND so.next_due_at <= NOW()
           AND so.stripe_payment_method_id IS NOT NULL
           AND u.stripe_customer_id IS NOT NULL
         LIMIT 500",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "process_standing_orders: query failed");
            return;
        }
    };

    info!(count = orders.len(), "processing standing orders");

    for order in orders {
        charge_standing_order(state, &order).await;
    }
}

async fn charge_standing_order(state: &AppState, order: &StandingOrder) {
    let so_id   = order.id;
    let user_id = order.user_id;

    let pi_result = state
        .stripe()
        .charge_off_session(
            order.total_cents,
            "cad",
            order.stripe_customer_id.as_str(),
            &order.stripe_payment_method_id,
            &[
                ("type",             "standing_order"),
                ("standing_order_id", &so_id.to_string()),
                ("user_id",          &user_id.to_string()),
            ],
        )
        .await;

    match pi_result {
        Ok(pi) if pi.status == "succeeded" => {
            // Create the order row and advance next_due_at in a single transaction.
            let tx_result = fulfill_standing_order(state, order, &pi.id).await;
            if let Err(e) = tx_result {
                tracing::error!(so_id, error = %e, "standing order fulfillment failed after charge");
            } else {
                tracing::info!(so_id, user_id, "standing order charged and fulfilled");
                send_standing_order_email(state, order).await;
            }
        }
        Ok(pi) => {
            // Payment requires action or was declined — record the failure.
            tracing::warn!(so_id, status = %pi.status, "standing order charge not succeeded");
            record_standing_order_failure(state, so_id).await;
        }
        Err(e) => {
            tracing::error!(so_id, error = %e, "standing order Stripe charge failed");
            record_standing_order_failure(state, so_id).await;
        }
    }
}

async fn fulfill_standing_order(
    state:    &AppState,
    order:    &StandingOrder,
    pi_id:    &str,
) -> Result<(), sqlx::Error> {
    let mut tx = state.db.begin().await?;

    // Insert the new order.
    sqlx::query(
        "INSERT INTO orders
             (user_id, variety_id, location_id, quantity, total_cents,
              stripe_payment_intent_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, 'paid')",
    )
    .bind(order.user_id)
    .bind(order.variety_id)
    .bind(order.location_id)
    .bind(order.quantity)
    .bind(order.total_cents)
    .bind(pi_id)
    .execute(&mut *tx)
    .await?;

    // Advance next_due_at based on frequency and reset failure_count.
    sqlx::query(
        "UPDATE standing_orders
         SET next_due_at = next_due_at + CASE frequency
                 WHEN 'weekly'  THEN INTERVAL '7 days'
                 WHEN 'monthly' THEN INTERVAL '1 month'
                 ELSE                INTERVAL '1 day'
             END,
             failure_count = 0
         WHERE id = $1",
    )
    .bind(order.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn record_standing_order_failure(state: &AppState, so_id: i32) {
    // After 3 consecutive failures, pause the standing order so the user can
    // update their payment method without being silently recharged on retry.
    let _ = sqlx::query(
        "UPDATE standing_orders
         SET failure_count = failure_count + 1,
             status = CASE WHEN failure_count + 1 >= 3 THEN 'paused' ELSE status END
         WHERE id = $1",
    )
    .bind(so_id)
    .execute(&state.db)
    .await;
}

async fn send_standing_order_email(state: &AppState, order: &StandingOrder) {
    let Some(ref key) = state.cfg.resend_api_key else { return };
    let Some(ref email) = order.user_email else { return };

    let http    = state.http.clone();
    let key     = key.expose_secret().to_owned();
    let email   = email.clone();
    let variety = order.variety_name.clone();
    let total   = order.total_cents as i32;
    let oid     = order.id; // use standing_order id as reference; real order_id from DB insert is opaque here

    tokio::spawn(async move {
        let _ = resend::send_order_confirmation(&http, &key, &email, oid, &variety, total).await;
    });
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
        let _ = sqlx::query(
            "UPDATE memberships
             SET renewal_notified = true
             WHERE status = 'active'
               AND renews_at IS NOT NULL
               AND renews_at <= NOW() + INTERVAL '30 days'
               AND renewal_notified = false",
        )
        .execute(&state.db)
        .await;
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
    let _ = resend::send(http, key, email, &subject, &html).await;
}

// ── January reset ─────────────────────────────────────────────────────────────

async fn january_reset(pool: &PgPool) {
    if let Err(e) = sqlx::query("UPDATE membership_funds SET balance_cents = 0")
        .execute(pool)
        .await
    {
        tracing::error!(error = %e, "january_reset: fund reset failed");
    }

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
