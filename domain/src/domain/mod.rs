/// Authentication: Apple Sign In, magic link, JWT management.
pub mod auth;
/// Dorotka AI assistant: system prompts and input sanitisation.
pub mod dorotka;
/// X3DH key bundle management: registration, claims, one-time pre-keys.
pub mod keys;
/// End-to-end encrypted messaging: send, thread, conversations.
pub mod messages;
/// User profiles and search.
pub mod users;
