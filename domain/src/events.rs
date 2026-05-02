use crate::types::UserId;

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

    // ── Businesses ────────────────────────────────────────────────────────────
    /// A new business was registered on the platform.
    BusinessCreated {
        /// The newly created business's database ID.
        business_id: i32,
        /// The user who created the business.
        user_id: i32,
    },

    // ── Beacons ───────────────────────────────────────────────────────────────
    /// A new beacon was registered at a business location.
    BeaconCreated {
        /// The newly created beacon's database ID.
        beacon_id: i32,
        /// The business this beacon belongs to.
        business_id: i32,
        /// The user who created the beacon.
        user_id: i32,
    },
    /// A beacon's secret key was rotated.
    BeaconKeyRotated {
        /// The beacon whose key was rotated.
        beacon_id: i32,
        /// The user who triggered the rotation.
        user_id: i32,
    },

    // ── Presence ─────────────────────────────────────────────────────────────
    /// A user's presence threshold was met (3 events on 3 days).
    PresenceThresholdMet {
        /// The user who met the threshold.
        user_id: i32,
        /// The business where presence was established.
        business_id: i32,
    },
    /// A presence event was recorded (qualifying or non-qualifying).
    PresenceEventRecorded {
        /// The user the event belongs to.
        user_id: i32,
        /// "beacon_dwell" or "nfc_tap".
        event_type: String,
        /// Whether the event counts toward the threshold.
        is_qualifying: bool,
    },

    // ── Dorotka ───────────────────────────────────────────────────────────────
    /// A query was submitted to the Dorotka AI assistant.
    DorotkaQueried {
        /// The platform context ("fraise" or "whisked").
        context: String,
    },
}
