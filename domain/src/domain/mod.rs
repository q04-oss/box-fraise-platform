/// Staff attestations — initiation, co-signing, approval, and rejection (BFIP Section 6).
pub mod attestations;
/// Authentication: Apple Sign In, magic link, JWT management.
pub mod auth;
/// Background checks — sanctions, identity fraud, and criminal screening (BFIP Sections 3b, 7b).
pub mod background_checks;
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
/// Staff roles, visit lifecycle, and quality assessments (BFIP Sections 6, 10, 12.3).
pub mod staff;
/// User profiles and search.
pub mod users;
