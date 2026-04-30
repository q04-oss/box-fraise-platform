-- Migration 006: NFC cup stickers
--
-- Each physical NFC sticker has a permanent UUID written at manufacture.
-- The same UUID is printed as a companion QR code on the sticker label.
-- Staff scan the companion QR via the staff web app to activate the sticker
-- for 2 hours before handing the cup to the customer.
-- The customer taps the sticker with their phone — iOS/Android opens the URL
-- automatically — the app intercepts it as a Universal Link and redeems the steep.
--
-- Stickers are registered to a business on first activation. Subsequent
-- activations from a different business are rejected (a sticker belongs to
-- one venue). total_taps provides basic analytics and abuse detection.

CREATE TABLE nfc_stickers (
    id          BIGSERIAL    PRIMARY KEY,
    uuid        TEXT         NOT NULL UNIQUE,
    business_id INTEGER      NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    total_taps  INTEGER      NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_nfc_stickers_business ON nfc_stickers (business_id);
