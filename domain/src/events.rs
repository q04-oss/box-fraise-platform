use crate::types::{MessageId, UserId};

#[derive(Debug, Clone)]
pub enum DomainEvent {
    // ── Auth ──────────────────────────────────────────────────────────────────
    UserRegistered { user_id: UserId, email: String },
    UserLoggedIn   { user_id: UserId },
    EmailVerified  { user_id: UserId },
    PasswordReset  { user_id: UserId },

    // ── Messages ──────────────────────────────────────────────────────────────
    MessageSent {
        message_id:   MessageId,
        sender_id:    UserId,
        recipient_id: UserId,
    },

    // ── Keys ──────────────────────────────────────────────────────────────────
    KeyBundleRegistered { user_id: UserId },
    KeyBundleDepleted   { user_id: UserId },
}
