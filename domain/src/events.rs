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

    // ── Identity credentials ─────────────────────────────────────────────────
    /// A Stripe Identity verification was recorded and cooling period started.
    IdentityVerificationInitiated {
        /// The user who initiated verification.
        user_id: i32,
        /// The newly created credential's database ID.
        credential_id: i32,
    },
    /// A qualifying app open was recorded during the cooling period.
    CoolingAppOpenRecorded {
        /// The user who opened the app.
        user_id: i32,
        /// The credential the open was recorded against.
        credential_id: i32,
        /// Number of distinct qualifying days so far.
        days_completed: i64,
    },
    /// The cooling period completed (all day + time requirements met).
    CoolingPeriodCompleted {
        /// The user whose cooling period completed.
        user_id: i32,
        /// The credential that was completed.
        credential_id: i32,
    },

    // ── Attestations ─────────────────────────────────────────────────────────
    /// A staff attestation was initiated for a user.
    AttestationInitiated {
        attestation_id: i32,
        user_id:        i32,
        visit_id:       i32,
    },
    /// A staff attestation was approved (both reviewers co-signed).
    AttestationApproved {
        attestation_id: i32,
        user_id:        i32,
    },
    /// A staff attestation was rejected by a reviewer.
    AttestationRejected {
        attestation_id:        i32,
        user_id:               i32,
        rejection_reviewer_id: i32,
    },

    // ── Staff ─────────────────────────────────────────────────────────────────
    /// A staff role was granted to a user.
    StaffRoleGranted {
        user_id: i32,
        role:    String,
    },
    /// A staff visit was scheduled.
    VisitScheduled {
        visit_id:    i32,
        location_id: i32,
    },
    /// A staff visit was marked completed.
    VisitCompleted {
        visit_id: i32,
    },
    /// A quality assessment was submitted for a business.
    QualityAssessmentSubmitted {
        visit_id:     i32,
        business_id:  i32,
        overall_pass: bool,
    },

    // ── Background checks ─────────────────────────────────────────────────────
    /// A background check was initiated.
    BackgroundCheckInitiated {
        user_id:    i32,
        check_id:   i32,
        check_type: String,
    },
    /// A background check returned a passed result.
    BackgroundCheckPassed {
        user_id:    i32,
        check_id:   i32,
        check_type: String,
    },
    /// A background check returned a failed result.
    BackgroundCheckFailed {
        user_id:    i32,
        check_id:   i32,
        check_type: String,
    },

    // ── Dorotka ───────────────────────────────────────────────────────────────
    /// A query was submitted to the Dorotka AI assistant.
    DorotkaQueried {
        /// The platform context ("fraise" or "whisked").
        context: String,
    },
}
