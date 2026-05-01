#![no_main]
//! Fuzz target: HMAC signing and constant-time comparison.
//!
//! Confirms that no combination of key bytes, message bytes, or comparison
//! bytes causes a panic. Ring's HMAC_SHA256 must be stable under all inputs.
//!
//! Run: cargo +nightly fuzz run hmac_verify

use base64::{engine::general_purpose::STANDARD, Engine};
use libfuzzer_sys::fuzz_target;
use ring::hmac;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Use the first byte to determine split point for key vs message.
    let split = (data[0] as usize % data.len()).max(1);
    let (key_bytes, msg) = data.split_at(split);

    // Sign the message.
    let key    = hmac::Key::new(hmac::HMAC_SHA256, key_bytes);
    let sig    = hmac::sign(&key, msg);
    let sig_b64 = STANDARD.encode(sig.as_ref());

    // Constant-time compare signature against the raw message —
    // they will never match but must never panic.
    let _ = sig_b64
        .as_bytes()
        .iter()
        .zip(msg.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y));
});
