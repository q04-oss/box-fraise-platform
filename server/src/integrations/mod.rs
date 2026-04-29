// External service clients are added here as the domains that need them are ported.
//
// Planned:
//   stripe.rs           — Stripe REST API (payment intents, webhooks, transfers)
//   apple.rs            — Apple Sign In JWKS + App Attest verification
//   expo_push.rs        — Expo push notification delivery
//   resend.rs           — Transactional email via Resend
//   anthropic.rs        — Claude AI (AI endpoints, gift notes, poetry)
//   cloudinary.rs       — Image upload + CDN
//   outbound_webhooks.rs— HMAC-signed outbound webhooks to subscribers
