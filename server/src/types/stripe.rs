/// Stripe API identifier newtypes.
///
/// Stripe IDs have a documented prefix format (`cus_`, `sub_`, `pi_`).
/// The validating constructors here prevent accidentally storing or sending
/// a Stripe ID of the wrong type — e.g. binding a payment-intent ID where a
/// customer ID is expected would fail at Stripe's API and be hard to diagnose.
///
/// The inner `String` is private. Construction goes through the prefix-
/// validating `new()` constructor. sqlx and serde treat these transparently.
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
#[error("invalid Stripe ID: expected `{prefix}` prefix, got `{value}`")]
pub struct InvalidStripeId {
    prefix: &'static str,
    value:  String,
}

// ── Macro ─────────────────────────────────────────────────────────────────────

macro_rules! stripe_id {
    (
        $(#[$attr:meta])*
        $name:ident, $prefix:literal
    ) => {
        $(#[$attr])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[derive(Serialize)]
        #[serde(transparent)]
        #[derive(sqlx::Type)]
        #[sqlx(transparent)]
        pub struct $name(String);

        impl $name {
            /// Construct, validating the expected Stripe prefix.
            /// Threat: accepting an arbitrary string bypasses Stripe's ID-type
            /// separation and can cause silent binding of the wrong resource.
            pub fn new(s: impl Into<String>) -> Result<Self, InvalidStripeId> {
                let s = s.into();
                if !s.starts_with($prefix) {
                    return Err(InvalidStripeId { prefix: $prefix, value: s });
                }
                Ok(Self(s))
            }

            pub fn as_str(&self) -> &str { &self.0 }
        }

        /// Validation runs on every deserialization — enforces the prefix
        /// constraint on values coming from JSON bodies or DB rows.
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::new(s).map_err(serde::de::Error::custom)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }

        impl FromStr for $name {
            type Err = InvalidStripeId;
            fn from_str(s: &str) -> Result<Self, Self::Err> { Self::new(s) }
        }
    };
}

// ── Types ─────────────────────────────────────────────────────────────────────

stripe_id!(
    /// Stripe customer ID — `cus_...`.
    StripeCustomerId,
    "cus_"
);

stripe_id!(
    /// Stripe subscription ID — `sub_...`.
    StripeSubscriptionId,
    "sub_"
);

stripe_id!(
    /// Stripe payment intent ID — `pi_...`.
    StripePaymentIntentId,
    "pi_"
);
