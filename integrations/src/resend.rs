/// Resend transactional email client.
///
/// All email functions are fire-and-forget at the call site — email delivery
/// failure must never roll back a successful business transaction.
use serde::Serialize;

const RESEND_API: &str = "https://api.resend.com/emails";
const FROM:       &str = "Maison Fraise <hello@fraise.box>";

// ── Core send ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SendRequest<'a> {
    from:    &'a str,
    to:      Vec<&'a str>,
    subject: &'a str,
    html:    &'a str,
}

pub async fn send(
    http:    &reqwest::Client,
    api_key: &str,
    to:      &str,
    subject: &str,
    html:    &str,
) -> anyhow::Result<()> {
    let resp = http
        .post(RESEND_API)
        .bearer_auth(api_key)
        .json(&SendRequest { from: FROM, to: vec![to], subject, html })
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Resend request failed: {e}"))?;

    if !resp.status().is_success() {
        tracing::warn!(to, status = %resp.status(), "email delivery failed");
    }

    Ok(())
}

// ── Email templates ───────────────────────────────────────────────────────────

pub async fn send_verification_email(
    http:       &reqwest::Client,
    api_key:    &str,
    to:         &str,
    verify_url: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        r#"<p>Verify your email to start earning loyalty steeps.</p>
           <p style="margin:32px 0">
             <a href="{verify_url}"
                style="background:#C9973A;color:#0a0a0a;padding:12px 24px;
                       text-decoration:none;border-radius:6px;font-size:14px;
                       font-family:-apple-system,sans-serif;font-weight:600">
               verify email
             </a>
           </p>
           <p style="color:#555;font-size:12px">
             This link expires in 24 hours. If you didn't create an account, ignore this email.
           </p>"#
    ));
    send(http, api_key, to, "verify your email — box fraise", &html).await
}

pub async fn send_password_reset(
    http:      &reqwest::Client,
    api_key:   &str,
    to:        &str,
    reset_url: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        r#"<p>We received a request to reset your Box Fraise password.</p>
           <p style="margin:32px 0">
             <a href="{reset_url}"
                style="background:#C9973A;color:#0a0a0a;padding:12px 24px;
                       text-decoration:none;border-radius:6px;font-size:14px;
                       font-family:-apple-system,sans-serif;font-weight:600">
               reset password
             </a>
           </p>
           <p style="color:#555;font-size:12px">
             This link expires in 1 hour. If you didn't request this, ignore the email —
             your password hasn't changed.
           </p>"#
    ));
    send(http, api_key, to, "reset your password — box fraise", &html).await
}

pub async fn send_order_confirmation(
    http:      &reqwest::Client,
    api_key:   &str,
    to:        &str,
    order_ref: impl std::fmt::Display,
    variety:   &str,
    total_cents: i32,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p>Your order #{order_ref} for <strong>{variety}</strong> has been placed.</p>
         <p>Total: <strong>{}</strong></p>
         <p>We'll notify you when your box is ready.</p>",
        format_cents(total_cents),
    ));
    send(http, api_key, to, "Order confirmed — Maison Fraise", &html).await
}

pub async fn send_order_ready(
    http:    &reqwest::Client,
    api_key: &str,
    to:      &str,
    order_id: i32,
    location: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p>Your box is ready for collection at <strong>{location}</strong>.</p>
         <p>Order #{order_id}</p>",
    ));
    send(http, api_key, to, "Your box is ready — Maison Fraise", &html).await
}

pub async fn send_order_queued(
    http:    &reqwest::Client,
    api_key: &str,
    to:      &str,
    variety: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p>Your order for <strong>{variety}</strong> has been added to the next batch.</p>
         <p>You'll be charged only when the batch is confirmed.</p>",
    ));
    send(http, api_key, to, "Added to batch — Maison Fraise", &html).await
}

pub async fn send_rsvp_confirmed(
    http:     &reqwest::Client,
    api_key:  &str,
    to:       &str,
    event:    &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p>Your RSVP for <strong>{event}</strong> is confirmed.</p>",
    ));
    send(http, api_key, to, &format!("RSVP confirmed — {event}"), &html).await
}

pub async fn send_gift_notification(
    http:       &reqwest::Client,
    api_key:    &str,
    to:         &str,
    from_name:  &str,
    claim_token: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p><strong>{from_name}</strong> sent you a gift.</p>
         <p>Claim code: <strong>{claim_token}</strong></p>",
    ));
    send(http, api_key, to, "You received a gift — Maison Fraise", &html).await
}

pub async fn send_nomination(
    http:     &reqwest::Client,
    api_key:  &str,
    to:       &str,
    event:    &str,
    nominator: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p><strong>{nominator}</strong> nominated you for <strong>{event}</strong>.</p>",
    ));
    send(http, api_key, to, &format!("You've been nominated — {event}"), &html).await
}

pub async fn send_contract_offer(
    http:        &reqwest::Client,
    api_key:     &str,
    to:          &str,
    business:    &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p>You have a new placement offer from <strong>{business}</strong>.</p>
         <p>Open the app to accept or decline.</p>",
    ));
    send(http, api_key, to, &format!("Placement offer — {business}"), &html).await
}

pub async fn send_tip_received(
    http:       &reqwest::Client,
    api_key:    &str,
    to:         &str,
    amount_cents: i32,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        "<p>You received a tip of <strong>{}</strong>.</p>",
        format_cents(amount_cents),
    ));
    send(http, api_key, to, "Tip received — Maison Fraise", &html).await
}

pub async fn send_magic_link_email(
    http:     &reqwest::Client,
    api_key:  &str,
    to:       &str,
    link_url: &str,
) -> anyhow::Result<()> {
    let html = base_template(&format!(
        r#"<p>Tap the button below to sign in. This link expires in 15 minutes.</p>
           <p style="margin:32px 0">
             <a href="{link_url}"
                style="background:#C9973A;color:#0a0a0a;padding:12px 24px;
                       text-decoration:none;border-radius:6px;font-size:14px;
                       font-family:-apple-system,sans-serif;font-weight:600">
               sign in
             </a>
           </p>
           <p style="color:#555;font-size:12px">
             If you didn't request this, you can safely ignore it.
           </p>"#
    ));
    send(http, api_key, to, "sign in to Whisked", &html).await
}

pub fn renewal_reminder_html(
    tier:      &str,
    renews_at: chrono::NaiveDateTime,
    days_left: i64,
) -> String {
    base_template(&format!(
        "<p>Your <strong>{tier}</strong> membership renews in <strong>{days_left} day{s}</strong>.</p>
         <p>Renewal date: <strong>{date}</strong></p>
         <p>Open the app to manage your membership.</p>",
        s    = if days_left == 1 { "" } else { "s" },
        date = renews_at.format("%B %-d, %Y"),
    ))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn format_cents(cents: i32) -> String {
    format!("CA${:.2}", cents as f64 / 100.0)
}

fn base_template(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<body style="background:#0a0a0a;color:#f5f5f0;font-family:Georgia,serif;padding:40px;max-width:560px;margin:0 auto">
  <p style="color:#C9973A;font-size:11px;letter-spacing:.2em;text-transform:uppercase;margin-bottom:32px">
    Maison Fraise
  </p>
  {body}
  <hr style="border:none;border-top:1px solid #222;margin:40px 0"/>
  <p style="color:#555;font-size:11px">
    fraise.box &mdash; Questions? Reply to this email.
  </p>
</body>
</html>"#
    )
}
