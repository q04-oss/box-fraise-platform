/// Authentication: Apple Sign In, magic link, JWT management.
pub mod auth;
/// BLE beacons — HMAC-based daily UUID derivation and key rotation.
pub mod beacons;
/// Presence verification — beacon dwell and NFC tap threshold tracking.
pub mod presence;
/// Partner businesses — registration, status, and location.
pub mod businesses;
/// Identity verification (Stripe Identity) and cooling period tracking.
pub mod identity_credentials;
/// Dorotka AI assistant: system prompts and input sanitisation.
pub mod dorotka;
/// User profiles and search.
pub mod users;
