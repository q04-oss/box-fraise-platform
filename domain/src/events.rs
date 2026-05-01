use crate::types::{MessageId, UserId};

/// All significant state-change events that the platform can broadcast.
///
/// Published by service functions via [`crate::event_bus::EventBus::publish`]
/// and consumed by background tasks (audit logging, push notifications, etc.).
/// Consumers that miss events receive [`tokio::sync::broadcast::error::RecvError::Lagged`].
#[derive(Debug, Clone)]
pub enum DomainEvent {
    // ── Auth ──────────────────────────────────────────────────────────────────
    /// A new user account was created.
    UserRegistered {
        /// The newly created user's identifier.
        user_id: UserId,
        /// Email address used during registration.
        email: String,
    },
    /// A user successfully authenticated.
    UserLoggedIn {
        /// The authenticated user's identifier.
        user_id: UserId,
    },

    // ── Messages ──────────────────────────────────────────────────────────────
    /// A message was sent between two users.
    MessageSent {
        /// Identifier of the newly created message.
        message_id:   MessageId,
        /// User who sent the message.
        sender_id:    UserId,
        /// User who received the message.
        recipient_id: UserId,
    },

    // ── Keys ──────────────────────────────────────────────────────────────────
    /// A user registered or updated their X3DH key bundle.
    KeyBundleRegistered {
        /// The user whose keys were registered.
        user_id: UserId,
    },
    /// A user's one-time pre-key supply has been exhausted.
    ///
    /// The key owner should upload new pre-keys immediately.
    KeyBundleDepleted {
        /// The user who needs to upload new pre-keys.
        user_id: UserId,
    },
}
