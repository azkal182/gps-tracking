CREATE TABLE memberships (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    role TEXT NOT NULL CHECK (role IN ('organization_admin', 'operator', 'viewer')),
    status TEXT NOT NULL DEFAULT 'pending_mfa'
        CHECK (status IN ('pending_mfa', 'active', 'suspended')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (tenant_id, user_id),
    UNIQUE (tenant_id, id)
);
CREATE INDEX memberships_user_tenant_idx ON memberships (user_id, tenant_id);
CREATE INDEX memberships_tenant_status_idx ON memberships (tenant_id, status, role);

CREATE TABLE invitations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    email_canonical TEXT NOT NULL
        CHECK (email_canonical = lower(btrim(email_canonical)))
        CHECK (char_length(email_canonical) BETWEEN 3 AND 320),
    role TEXT NOT NULL CHECK (role IN ('organization_admin', 'operator', 'viewer')),
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'revoked', 'expired')),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (expires_at > created_at),
    CHECK (consumed_at IS NULL OR revoked_at IS NULL),
    CHECK ((status = 'accepted') = (consumed_at IS NOT NULL)),
    CHECK ((status = 'revoked') = (revoked_at IS NOT NULL))
);
CREATE UNIQUE INDEX invitations_one_pending_email_idx
    ON invitations (tenant_id, email_canonical) WHERE status = 'pending';
CREATE INDEX invitations_expiry_idx ON invitations (expires_at) WHERE status = 'pending';

CREATE TABLE auth_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    secret_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(secret_hash) = 32),
    csrf_hash BYTEA NOT NULL CHECK (octet_length(csrf_hash) = 32),
    auth_version BIGINT NOT NULL CHECK (auth_version >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    last_interactive_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    expires_at TIMESTAMPTZ NOT NULL,
    reauthenticated_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > created_at),
    CHECK (last_interactive_at >= created_at),
    CHECK (reauthenticated_at IS NULL OR reauthenticated_at >= created_at)
);
CREATE INDEX auth_sessions_user_active_idx ON auth_sessions (user_id, expires_at) WHERE revoked_at IS NULL;
CREATE INDEX auth_sessions_expiry_idx ON auth_sessions (expires_at) WHERE revoked_at IS NULL;

CREATE TABLE auth_challenges (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    secret_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(secret_hash) = 32),
    purpose TEXT NOT NULL CHECK (purpose IN ('mfa_login', 'mfa_enrollment', 'mfa_recovery')),
    expires_at TIMESTAMPTZ NOT NULL,
    attempt_count SMALLINT NOT NULL DEFAULT 0 CHECK (attempt_count BETWEEN 0 AND 5),
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (expires_at > created_at)
);
CREATE INDEX auth_challenges_expiry_idx ON auth_challenges (expires_at) WHERE consumed_at IS NULL;

CREATE TABLE mfa_factors (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    encrypted_secret BYTEA NOT NULL CHECK (octet_length(encrypted_secret) >= 16),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) BETWEEN 12 AND 24),
    key_version SMALLINT NOT NULL CHECK (key_version > 0),
    enabled_at TIMESTAMPTZ,
    pending_expires_at TIMESTAMPTZ,
    last_accepted_step BIGINT CHECK (last_accepted_step IS NULL OR last_accepted_step >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((enabled_at IS NULL) = (pending_expires_at IS NOT NULL))
);
CREATE UNIQUE INDEX mfa_factors_one_enabled_user_idx ON mfa_factors (user_id) WHERE enabled_at IS NOT NULL;
CREATE UNIQUE INDEX mfa_factors_one_pending_user_idx ON mfa_factors (user_id) WHERE pending_expires_at IS NOT NULL;

CREATE TABLE recovery_codes (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(code_hash) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    consumed_at TIMESTAMPTZ
);
CREATE INDEX recovery_codes_user_available_idx ON recovery_codes (user_id, created_at) WHERE consumed_at IS NULL;

CREATE TABLE account_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose TEXT NOT NULL CHECK (purpose IN ('password_reset', 'activation', 'recovery')),
    secret_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(secret_hash) = 32),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (expires_at > created_at),
    CHECK (consumed_at IS NULL OR revoked_at IS NULL)
);
CREATE UNIQUE INDEX account_tokens_one_active_purpose_idx
    ON account_tokens (user_id, purpose) WHERE consumed_at IS NULL AND revoked_at IS NULL;
CREATE INDEX account_tokens_expiry_idx ON account_tokens (expires_at) WHERE consumed_at IS NULL AND revoked_at IS NULL;

CREATE TABLE email_outbox (
    id UUID PRIMARY KEY,
    tenant_id UUID REFERENCES organizations(id) ON DELETE RESTRICT,
    recipient TEXT NOT NULL CHECK (char_length(recipient) BETWEEN 3 AND 320),
    template TEXT NOT NULL CHECK (template IN ('invitation', 'password_reset', 'account_activation', 'security_notice')),
    encrypted_payload BYTEA NOT NULL CHECK (octet_length(encrypted_payload) > 0),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sending', 'sent', 'failed')),
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 20),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    sent_at TIMESTAMPTZ,
    retention_expires_at TIMESTAMPTZ NOT NULL,
    CHECK (retention_expires_at > created_at AND retention_expires_at <= created_at + INTERVAL '72 hours'),
    CHECK ((status = 'sent') = (sent_at IS NOT NULL))
);
CREATE INDEX email_outbox_pending_idx ON email_outbox (next_attempt_at, created_at) WHERE status IN ('pending', 'failed');
CREATE INDEX email_outbox_retention_idx ON email_outbox (retention_expires_at);
