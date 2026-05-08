//! Notification events broadcast to SSE subscribers (Hardening §7).
//!
//! Domain events flow through the existing `event_bus`; this module owns the
//! *outbound* notification shape — the concrete JSON the iOS / web client
//! receives over `/api/notifications/stream`. Keep the variant set narrow:
//! every variant is a UI signal worth interrupting the user for.
//!
//! `target_user_id() == 0` is the broadcast sentinel (every connected
//! subscriber sees the event); any non-zero value is delivered to that
//! user only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEvent {
    DeliveryRevealed {
        visit_id:     i32,
        business_id:  i32,
        window_start: chrono::DateTime<chrono::Utc>,
        window_hours: i32,
    },
    AttestationApproved {
        attestation_id: i32,
        user_id:        i32,
    },
    AttestationRejected {
        attestation_id: i32,
        user_id:        i32,
        reason:         Option<String>,
    },
    OrderReady {
        order_id:    i32,
        user_id:     i32,
        business_id: i32,
    },
    SupportBookingConfirmed {
        booking_id: i32,
        user_id:    i32,
        visit_id:   i32,
    },
    BackgroundCheckResult {
        user_id:    i32,
        check_type: String,
        status:     String,
    },
    SoultokenIssued {
        user_id:      i32,
        display_code: String,
    },
}

impl NotificationEvent {
    /// User the event should reach. `0` means broadcast to every connected
    /// subscriber; non-zero values are delivered only to that user.
    pub fn target_user_id(&self) -> i32 {
        match self {
            Self::DeliveryRevealed { .. }                => 0,
            Self::AttestationApproved { user_id, .. }    => *user_id,
            Self::AttestationRejected { user_id, .. }    => *user_id,
            Self::OrderReady { user_id, .. }             => *user_id,
            Self::SupportBookingConfirmed { user_id, .. }=> *user_id,
            Self::BackgroundCheckResult { user_id, .. }  => *user_id,
            Self::SoultokenIssued { user_id, .. }        => *user_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_event_serialises_correctly() {
        let event = NotificationEvent::AttestationApproved {
            attestation_id: 42,
            user_id:        7,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"],           "attestation_approved");
        assert_eq!(json["attestation_id"], 42);
        assert_eq!(json["user_id"],        7);
    }

    #[test]
    fn target_user_id_returns_correct_user() {
        // Broadcast variant
        let delivery = NotificationEvent::DeliveryRevealed {
            visit_id:     1,
            business_id:  2,
            window_start: chrono::Utc::now(),
            window_hours: 4,
        };
        assert_eq!(delivery.target_user_id(), 0,
            "DeliveryRevealed must broadcast (target_user_id == 0)");

        // Per-user variants — every variant carries a user_id and must surface it.
        let approved = NotificationEvent::AttestationApproved { attestation_id: 1, user_id: 11 };
        assert_eq!(approved.target_user_id(), 11);

        let rejected = NotificationEvent::AttestationRejected {
            attestation_id: 1, user_id: 12, reason: None,
        };
        assert_eq!(rejected.target_user_id(), 12);

        let order = NotificationEvent::OrderReady { order_id: 1, user_id: 13, business_id: 1 };
        assert_eq!(order.target_user_id(), 13);

        let booking = NotificationEvent::SupportBookingConfirmed {
            booking_id: 1, user_id: 14, visit_id: 1,
        };
        assert_eq!(booking.target_user_id(), 14);

        let bg = NotificationEvent::BackgroundCheckResult {
            user_id: 15, check_type: "sanctions".to_owned(), status: "passed".to_owned(),
        };
        assert_eq!(bg.target_user_id(), 15);

        let soul = NotificationEvent::SoultokenIssued {
            user_id: 16, display_code: "AAAA-BBBB-CCCC".to_owned(),
        };
        assert_eq!(soul.target_user_id(), 16);
    }

    // ── Channel-delivery tests (cleanup task 5) ──────────────────────────────

    use tokio::sync::broadcast;

    fn make_channel() -> (
        broadcast::Sender<NotificationEvent>,
        broadcast::Receiver<NotificationEvent>,
    ) {
        broadcast::channel(16)
    }

    /// Round-trip: a published `AttestationApproved` reaches a subscriber on
    /// the same channel with payload intact. Smoke test for the wiring —
    /// any broken `Clone`/`Send` bound on `NotificationEvent` would surface
    /// here, not in the (otherwise identical) variant-shape tests above.
    #[tokio::test]
    async fn attestation_approved_event_reaches_subscriber() {
        let (tx, mut rx) = make_channel();

        tx.send(NotificationEvent::AttestationApproved {
            attestation_id: 42,
            user_id:        1,
        }).expect("at least one receiver — rx is in scope");

        let event = rx.recv().await.expect("recv");
        match event {
            NotificationEvent::AttestationApproved { attestation_id, user_id } => {
                assert_eq!(attestation_id, 42);
                assert_eq!(user_id, 1);
            }
            other => panic!("wrong event variant: {other:?}"),
        }
    }

    /// `SoultokenIssued` carries `display_code` through the channel
    /// (cleanup #6 plumbed this end-to-end; this asserts the wire payload
    /// at the channel boundary specifically — DomainEvent → handler →
    /// NotificationEvent → SSE).
    #[tokio::test]
    async fn soultoken_issued_event_carries_display_code() {
        let (tx, mut rx) = make_channel();

        tx.send(NotificationEvent::SoultokenIssued {
            user_id:      1,
            display_code: "ABCD-EFGH-IJKL".to_owned(),
        }).expect("at least one receiver");

        let event = rx.recv().await.expect("recv");
        match event {
            NotificationEvent::SoultokenIssued { user_id, display_code } => {
                assert_eq!(user_id, 1);
                assert_eq!(display_code, "ABCD-EFGH-IJKL");
            }
            other => panic!("wrong event variant: {other:?}"),
        }
    }

    /// User-scoped filtering: an event whose `target_user_id()` is user_a's id
    /// must NOT match user_b's filter `(target == user_b || target == 0)`.
    /// This is the exact predicate the SSE handler uses to decide whether to
    /// forward a received event to its connected client.
    #[tokio::test]
    async fn user_b_does_not_receive_user_a_events() {
        let (tx, mut rx) = make_channel();
        let user_a_id = 1;
        let user_b_id = 2;

        tx.send(NotificationEvent::AttestationApproved {
            attestation_id: 1,
            user_id:        user_a_id,
        }).expect("at least one receiver");

        let event = rx.recv().await.expect("recv");
        let target = event.target_user_id();

        // User B's filter, exactly as the SSE handler applies it.
        let user_b_should_receive = target == user_b_id || target == 0;
        assert!(
            !user_b_should_receive,
            "user_b filter must reject user_a's event (target_user_id = {target})",
        );
    }

    /// Broadcast variants (`target_user_id() == 0`) reach every subscriber.
    /// `tokio::sync::broadcast` fan-out is tested by tokio itself, but this
    /// pins down our intent: a 0-target event must not be filtered out by
    /// the per-user predicate for ANY subscriber.
    #[tokio::test]
    async fn broadcast_event_reaches_all_subscribers() {
        let (tx, mut rx1) = make_channel();
        let mut rx2 = tx.subscribe();

        tx.send(NotificationEvent::DeliveryRevealed {
            visit_id:     1,
            business_id:  1,
            window_start: chrono::Utc::now(),
            window_hours: 4,
        }).expect("two receivers");

        let event1 = rx1.recv().await.expect("rx1");
        let event2 = rx2.recv().await.expect("rx2");

        assert_eq!(event1.target_user_id(), 0, "rx1 sees broadcast sentinel");
        assert_eq!(event2.target_user_id(), 0, "rx2 sees broadcast sentinel");
    }
}
