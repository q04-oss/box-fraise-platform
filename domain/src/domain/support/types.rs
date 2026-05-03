#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SupportBookingRow {
    pub id:                           i32,
    pub visit_id:                     i32,
    pub user_id:                      i32,
    pub issue_description:            Option<String>,
    pub priority:                     String,
    pub status:                       String,
    pub booking_confirmation_sent_at: Option<DateTime<Utc>>,
    pub reminder_sent_at:             Option<DateTime<Utc>>,
    pub attended_at:                  Option<DateTime<Utc>>,
    pub cancelled_at:                 Option<DateTime<Utc>>,
    pub cancellation_reason:          Option<String>,
    pub rescheduled_to_visit_id:      Option<i32>,
    pub resolved_at:                  Option<DateTime<Utc>>,
    pub resolution_description:       Option<String>,
    pub resolution_staff_id:          Option<i32>,
    pub resolution_signature:         Option<String>,
    pub gift_box_provided:            bool,
    pub attestation_id:               Option<i32>,
    pub updated_at:                   DateTime<Utc>,
    pub created_at:                   DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GiftBoxHistoryRow {
    pub id:          i32,
    pub user_id:     i32,
    pub visit_id:    i32,
    pub box_id:      Option<i32>,
    pub gift_reason: String,
    pub covered_by:  String,
    pub gifted_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBookingRequest {
    pub visit_id:          i32,
    pub issue_description: Option<String>,
    pub priority:          Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveBookingRequest {
    pub resolution_description: String,
    pub resolution_signature:   String,
    pub gift_box_provided:      bool,
    pub gift_box_id:            Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CancelBookingRequest {
    pub cancellation_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupportBookingResponse {
    pub id:                i32,
    pub visit_id:          i32,
    pub priority:          String,
    pub status:            String,
    pub issue_description: Option<String>,
    pub gift_box_provided: bool,
    pub attended_at:       Option<DateTime<Utc>>,
    pub resolved_at:       Option<DateTime<Utc>>,
    pub created_at:        DateTime<Utc>,
}

pub const SUPPORT_BOOKING_COLS: &str =
    "id, visit_id, user_id, issue_description, priority, status, \
     booking_confirmation_sent_at, reminder_sent_at, attended_at, \
     cancelled_at, cancellation_reason, rescheduled_to_visit_id, \
     resolved_at, resolution_description, resolution_staff_id, \
     resolution_signature, gift_box_provided, attestation_id, \
     updated_at, created_at";
