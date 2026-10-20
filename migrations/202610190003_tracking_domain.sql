CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE targets (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 150),
    external_ref TEXT CHECK (
        external_ref IS NULL OR
        (char_length(btrim(external_ref)) BETWEEN 1 AND 128 AND octet_length(external_ref) <= 512)
    ),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (
        jsonb_typeof(metadata) = 'object' AND octet_length(metadata::text) <= 16384
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (tenant_id, id)
);
CREATE UNIQUE INDEX targets_external_ref_unique_idx
    ON targets (tenant_id, external_ref) WHERE external_ref IS NOT NULL;
CREATE INDEX targets_tenant_created_idx ON targets (tenant_id, created_at DESC, id DESC);

CREATE TABLE devices (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 150),
    protocol TEXT NOT NULL CHECK (char_length(protocol) BETWEEN 1 AND 64),
    identifier TEXT NOT NULL CHECK (char_length(identifier) BETWEEN 1 AND 128),
    ingestion_policy TEXT NOT NULL DEFAULT 'session_only'
        CHECK (ingestion_policy IN ('session_only', 'always')),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (
        jsonb_typeof(metadata) = 'object' AND octet_length(metadata::text) <= 16384
    ),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_contact_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, protocol, identifier)
);
CREATE INDEX devices_tenant_created_idx ON devices (tenant_id, created_at DESC, id DESC);
CREATE INDEX devices_tenant_last_contact_idx ON devices (tenant_id, last_contact_at DESC, id DESC);

CREATE TABLE device_credentials (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    device_id UUID NOT NULL,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    revoked_at TIMESTAMPTZ,
    FOREIGN KEY (tenant_id, device_id) REFERENCES devices (tenant_id, id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, id)
);
CREATE UNIQUE INDEX device_credentials_one_active_idx
    ON device_credentials (tenant_id, device_id) WHERE revoked_at IS NULL;
CREATE INDEX device_credentials_device_history_idx
    ON device_credentials (tenant_id, device_id, created_at DESC, id DESC);

CREATE TABLE application_tokens (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    owner_user_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 150),
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    scopes TEXT[] NOT NULL CHECK (
        cardinality(scopes) > 0 AND scopes <@ ARRAY[
            'targets:read', 'targets:write', 'devices:read', 'devices:write',
            'sessions:read', 'sessions:write', 'positions:read', 'tracking:subscribe'
        ]::TEXT[]
    ),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (tenant_id, owner_user_id) REFERENCES memberships (tenant_id, user_id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, id),
    CHECK (expires_at > created_at AND expires_at <= created_at + INTERVAL '90 days')
);
CREATE INDEX application_tokens_owner_idx
    ON application_tokens (tenant_id, owner_user_id, created_at DESC, id DESC);
CREATE INDEX application_tokens_expiry_idx
    ON application_tokens (expires_at) WHERE revoked_at IS NULL;

CREATE TABLE application_token_devices (
    tenant_id UUID NOT NULL,
    application_token_id UUID NOT NULL,
    device_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, application_token_id, device_id),
    FOREIGN KEY (tenant_id, application_token_id) REFERENCES application_tokens (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, device_id) REFERENCES devices (tenant_id, id) ON DELETE RESTRICT
);
CREATE INDEX application_token_devices_device_idx ON application_token_devices (tenant_id, device_id);

CREATE TABLE application_token_targets (
    tenant_id UUID NOT NULL,
    application_token_id UUID NOT NULL,
    target_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, application_token_id, target_id),
    FOREIGN KEY (tenant_id, application_token_id) REFERENCES application_tokens (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, target_id) REFERENCES targets (tenant_id, id) ON DELETE RESTRICT
);
CREATE INDEX application_token_targets_target_idx ON application_token_targets (tenant_id, target_id);

CREATE FUNCTION assert_application_token_has_allowlist()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    token_tenant UUID;
    token_id UUID;
BEGIN
    IF TG_TABLE_NAME = 'application_tokens' THEN
        token_tenant := COALESCE(NEW.tenant_id, OLD.tenant_id);
        token_id := COALESCE(NEW.id, OLD.id);
    ELSE
        token_tenant := COALESCE(NEW.tenant_id, OLD.tenant_id);
        token_id := COALESCE(NEW.application_token_id, OLD.application_token_id);
    END IF;

    IF EXISTS (
        SELECT 1 FROM application_tokens
        WHERE tenant_id = token_tenant AND id = token_id
    ) AND NOT EXISTS (
        SELECT 1 FROM application_token_devices
        WHERE tenant_id = token_tenant AND application_token_id = token_id
        UNION ALL
        SELECT 1 FROM application_token_targets
        WHERE tenant_id = token_tenant AND application_token_id = token_id
    ) THEN
        RAISE EXCEPTION 'application token requires a non-empty resource allowlist'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER application_tokens_require_allowlist
    AFTER INSERT OR UPDATE ON application_tokens
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_application_token_has_allowlist();
CREATE CONSTRAINT TRIGGER application_token_devices_preserve_allowlist
    AFTER INSERT OR UPDATE OR DELETE ON application_token_devices
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_application_token_has_allowlist();
CREATE CONSTRAINT TRIGGER application_token_targets_preserve_allowlist
    AFTER INSERT OR UPDATE OR DELETE ON application_token_targets
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_application_token_has_allowlist();

CREATE TABLE tracking_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    target_id UUID NOT NULL,
    device_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    ended_at TIMESTAMPTZ,
    session_period TSTZRANGE GENERATED ALWAYS AS (tstzrange(started_at, ended_at, '[)')) STORED,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (tenant_id, target_id) REFERENCES targets (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, device_id) REFERENCES devices (tenant_id, id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, id),
    CHECK (ended_at IS NULL OR ended_at >= started_at),
    EXCLUDE USING gist (tenant_id WITH =, device_id WITH =, session_period WITH &&)
);
CREATE UNIQUE INDEX tracking_sessions_one_active_target_idx
    ON tracking_sessions (tenant_id, target_id) WHERE ended_at IS NULL;
CREATE UNIQUE INDEX tracking_sessions_one_active_device_idx
    ON tracking_sessions (tenant_id, device_id) WHERE ended_at IS NULL;
CREATE INDEX tracking_sessions_target_start_idx
    ON tracking_sessions (tenant_id, target_id, started_at DESC, id DESC);
CREATE INDEX tracking_sessions_device_start_idx
    ON tracking_sessions (tenant_id, device_id, started_at DESC, id DESC);

CREATE FUNCTION position_expires_at_utc(recorded_at TIMESTAMPTZ)
RETURNS TIMESTAMPTZ
LANGUAGE SQL
IMMUTABLE
STRICT
PARALLEL SAFE
AS $$
    SELECT ((recorded_at AT TIME ZONE 'UTC') + INTERVAL '1 year') AT TIME ZONE 'UTC'
$$;

CREATE TABLE positions (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    device_id UUID NOT NULL,
    message_id UUID,
    payload_fingerprint BYTEA NOT NULL CHECK (octet_length(payload_fingerprint) = 32),
    location GEOGRAPHY(POINT, 4326) NOT NULL,
    altitude_m DOUBLE PRECISION,
    speed_mps DOUBLE PRECISION CHECK (speed_mps IS NULL OR speed_mps >= 0),
    heading_deg DOUBLE PRECISION CHECK (heading_deg IS NULL OR (heading_deg >= 0 AND heading_deg < 360)),
    accuracy_m DOUBLE PRECISION CHECK (accuracy_m IS NULL OR accuracy_m >= 0),
    recorded_at TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    expires_at TIMESTAMPTZ GENERATED ALWAYS AS (position_expires_at_utc(recorded_at)) STORED,
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (
        jsonb_typeof(attributes) = 'object' AND octet_length(attributes::text) <= 4096
    ),
    FOREIGN KEY (tenant_id, device_id) REFERENCES devices (tenant_id, id) ON DELETE RESTRICT,
    UNIQUE (device_id, message_id)
);
CREATE INDEX positions_latest_device_idx
    ON positions (tenant_id, device_id, recorded_at DESC, id DESC);
CREATE INDEX positions_expiry_purge_idx ON positions (expires_at, id);

CREATE TABLE idempotency_records (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    principal_id UUID NOT NULL,
    route_key TEXT NOT NULL CHECK (char_length(route_key) BETWEEN 1 AND 160),
    idempotency_key TEXT NOT NULL CHECK (char_length(idempotency_key) BETWEEN 1 AND 128),
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    response JSONB NOT NULL CHECK (jsonb_typeof(response) = 'object' AND octet_length(response::text) <= 16384),
    status_code SMALLINT NOT NULL CHECK (status_code BETWEEN 200 AND 299),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    expires_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, principal_id, route_key, idempotency_key),
    CHECK (expires_at > created_at AND expires_at <= created_at + INTERVAL '24 hours')
);
CREATE INDEX idempotency_expiry_idx ON idempotency_records (expires_at, id);

CREATE TABLE audit_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id UUID REFERENCES organizations(id) ON DELETE RESTRICT,
    actor_ref TEXT NOT NULL CHECK (char_length(actor_ref) BETWEEN 1 AND 160),
    action TEXT NOT NULL CHECK (char_length(action) BETWEEN 1 AND 128),
    resource_type TEXT,
    resource_id UUID,
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'denied', 'failure')),
    request_id TEXT NOT NULL CHECK (char_length(request_id) BETWEEN 1 AND 128),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((resource_type IS NULL) = (resource_id IS NULL)),
    CHECK (resource_type IS NULL OR char_length(resource_type) BETWEEN 1 AND 64)
);
CREATE INDEX audit_events_tenant_time_idx ON audit_events (tenant_id, occurred_at DESC, id DESC);
CREATE INDEX audit_events_time_idx ON audit_events (occurred_at DESC, id DESC);
