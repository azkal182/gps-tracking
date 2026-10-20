-- HTTP and future protocol adapters use opaque sender message IDs (up to 128 chars).
-- Preserve the existing per-device unique constraint while removing the UUID-only limit.
ALTER TABLE positions
    ALTER COLUMN message_id TYPE TEXT USING message_id::text;

ALTER TABLE positions
    ADD CONSTRAINT positions_message_id_length_check
    CHECK (message_id IS NULL OR (length(message_id) BETWEEN 1 AND 128));
