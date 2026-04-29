-- Server-issued App Attest challenges.
-- Each challenge is consumed atomically at attestation time to prevent
-- replay of old attestation objects.
CREATE TABLE IF NOT EXISTS attest_challenges (
    challenge  TEXT        PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '5 minutes'
);

-- Auto-clean expired challenges to bound table growth.
CREATE INDEX IF NOT EXISTS idx_attest_challenges_expires
    ON attest_challenges (expires_at);
