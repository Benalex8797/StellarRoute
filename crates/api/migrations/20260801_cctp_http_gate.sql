-- CCTP HTTP gate: transfer access capability hash + quote idempotency ledger.

ALTER TABLE cctp_transfers
    ADD COLUMN IF NOT EXISTS access_token_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_cctp_transfers_access_hash
    ON cctp_transfers (access_token_hash)
    WHERE access_token_hash IS NOT NULL;

CREATE TABLE IF NOT EXISTS cctp_quote_idempotency (
    idempotency_key TEXT PRIMARY KEY,
    request_hash TEXT NOT NULL,
    transfer_id UUID NOT NULL REFERENCES cctp_transfers(transfer_id) ON DELETE CASCADE,
    response_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cctp_quote_idempotency_expires
    ON cctp_quote_idempotency (expires_at);

ALTER TABLE cctp_quote_idempotency
    DROP CONSTRAINT IF EXISTS cctp_quote_idempotency_key_len;
ALTER TABLE cctp_quote_idempotency
    ADD CONSTRAINT cctp_quote_idempotency_key_len
    CHECK (char_length(idempotency_key) BETWEEN 1 AND 128);

ALTER TABLE cctp_quote_idempotency
    DROP CONSTRAINT IF EXISTS cctp_quote_idempotency_response_len;
ALTER TABLE cctp_quote_idempotency
    ADD CONSTRAINT cctp_quote_idempotency_response_len
    CHECK (char_length(response_json) BETWEEN 1 AND 16384);

COMMENT ON COLUMN cctp_transfers.access_token_hash IS
    'SHA-256 hex of one-time transfer access token; required for mutations/status.';
COMMENT ON TABLE cctp_quote_idempotency IS
    'Quote idempotency ledger; stores redacted wire responses for byte-identical replays.';
