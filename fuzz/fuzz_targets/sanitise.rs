#![no_main]
//! Fuzz target: input sanitiser.
//!
//! Feeds arbitrary byte sequences (including null bytes, control characters,
//! multi-byte Unicode, and very long inputs) through the sanitiser. The only
//! requirement is no panic — Err returns are acceptable.
//!
//! Run: cargo +nightly fuzz run sanitise

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only valid UTF-8 reaches the sanitiser — bytes that fail UTF-8 decode
    // would be rejected at the HTTP layer before service code sees them.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = box_fraise_domain::domain::dorotka::service::sanitise(s);
    }
});
