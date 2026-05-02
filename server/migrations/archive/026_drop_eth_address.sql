-- Migration 026: Drop users.eth_address.
--
-- The Ethereum wallet address feature was removed (PATCH /api/users/me/wallet
-- and its webhook handler). No Rust code reads or writes this column.
-- users_eth_address_key unique constraint drops automatically with the column.

ALTER TABLE users DROP COLUMN IF EXISTS eth_address;
