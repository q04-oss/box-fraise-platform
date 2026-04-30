-- Migration 003: Square OAuth tokens + Staff auth infrastructure
--
-- square_oauth_tokens: one row per business that has connected their Square
-- account. access_token and refresh_token are AES-256-GCM encrypted at the
-- application layer before insert. The plaintext never touches the DB wire.
--
-- The append-only constraint on this table is enforced by the application:
-- connect replaces an existing row via ON CONFLICT DO UPDATE, and the
-- encrypted values are never read back into a response — only decrypted
-- in-process for API calls.
--
-- Audit note: OAuth connect and token refresh events are written to the
-- audit_events table (created below) so incident responders can reconstruct
-- the authorization timeline for any business.

CREATE TABLE square_oauth_tokens (
    id                     BIGSERIAL    PRIMARY KEY,
    business_id            INTEGER      NOT NULL UNIQUE REFERENCES businesses(id) ON DELETE CASCADE,
    -- AES-256-GCM ciphertext, hex-encoded. Format: <12-byte nonce hex><ciphertext hex>
    -- Never returned in any API response. Never logged.
    encrypted_access_token  TEXT        NOT NULL,
    encrypted_refresh_token TEXT        NOT NULL,
    -- Square-assigned identifiers, safe to store in plaintext.
    merchant_id             TEXT        NOT NULL,
    square_location_id      TEXT        NOT NULL,
    expires_at              TIMESTAMPTZ NOT NULL,
    connected_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    refreshed_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Audit events ──────────────────────────────────────────────────────────────
-- Append-only log of security-relevant platform events. The trigger below
-- prevents UPDATE and DELETE so the audit trail cannot be scrubbed.
-- New event types are added by inserting rows with the new event_kind value —
-- no schema migration required.

CREATE TABLE audit_events (
    id          BIGSERIAL    PRIMARY KEY,
    -- The user or staff member who triggered the event. NULL for system events
    -- (e.g. Stripe webhooks, scheduled refreshes).
    actor_id    INTEGER      REFERENCES users(id) ON DELETE SET NULL,
    -- For staff actions: the business the staff member was acting on behalf of.
    business_id INTEGER      REFERENCES businesses(id) ON DELETE SET NULL,
    event_kind  TEXT         NOT NULL,
    -- Structured context — e.g. { "order_id": 42, "amount_cents": 1500 }.
    -- Never contains secrets or PII beyond IDs.
    metadata    JSONB        NOT NULL DEFAULT '{}',
    ip_address  INET,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- Trigger: reject any attempt to UPDATE or DELETE an audit row.
CREATE OR REPLACE FUNCTION audit_events_immutable()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_events rows are immutable';
END;
$$;

CREATE TRIGGER enforce_audit_immutability
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION audit_events_immutable();

-- Index for incident response queries: "all events for business X" or
-- "all events by actor Y in the last 24 hours".
CREATE INDEX idx_audit_business ON audit_events (business_id, created_at DESC)
    WHERE business_id IS NOT NULL;
CREATE INDEX idx_audit_actor ON audit_events (actor_id, created_at DESC)
    WHERE actor_id IS NOT NULL;
CREATE INDEX idx_audit_kind ON audit_events (event_kind, created_at DESC);
