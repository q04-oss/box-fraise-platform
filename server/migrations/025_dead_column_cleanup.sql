-- Migration 025: Drop dead columns with no Rust references.
--
-- orders.push_token: present in the initial schema but never included in
--   ORDER_COLS and never written or read by any handler. users.push_token
--   is the active notification token; orders.push_token was a legacy copy
--   that was never used after the notification architecture moved to users.

ALTER TABLE orders DROP COLUMN IF EXISTS push_token;
