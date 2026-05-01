/// Internal domain ID newtypes.
///
/// Every ID is a distinct type even though all underlying SQL types are INT4.
/// Threat defended: passing an `OrderId` where a `UserId` is expected (or
/// vice versa) is now a compile error rather than a silent data corruption bug.
///
/// The inner field is private. Construct from `i32` via `From`, extract via
/// `.get()` or `From<UserId> for i32`. The `From` impls exist for DB
/// boundary crossings and legacy call sites being migrated — prefer letting
/// sqlx decode directly to the newtype via `#[sqlx(transparent)]`.
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

macro_rules! int_id {
    (
        $(#[$attr:meta])*
        $name:ident
    ) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[derive(Serialize, Deserialize)]
        #[serde(transparent)]
        #[derive(sqlx::Type)]
        #[sqlx(transparent)]
        pub struct $name(i32);

        impl $name {
            /// Construct from a raw database value.
            /// Prefer letting sqlx decode directly via `#[sqlx(transparent)]`.
            #[inline]
            pub fn new(v: i32) -> Self { Self(v) }

            /// Extract the underlying `i32`.
            /// Use only at system boundaries (SQL parameter binding, legacy callers).
            #[inline]
            pub fn get(self) -> i32 { self.0 }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }

        impl FromStr for $name {
            type Err = std::num::ParseIntError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.parse::<i32>().map(Self)
            }
        }

        impl From<i32> for $name {
            fn from(v: i32) -> Self { Self(v) }
        }

        impl From<$name> for i32 {
            fn from(v: $name) -> i32 { v.0 }
        }
    };
}

int_id!(
    /// Identifies a row in the `users` table.
    UserId
);

int_id!(
    /// Identifies a row in the `orders` table.
    OrderId
);

int_id!(
    /// Identifies a row in the `messages` table.
    MessageId
);

int_id!(
    /// Identifies a one-time pre-key (`one_time_pre_keys.key_id`).
    KeyId
);
