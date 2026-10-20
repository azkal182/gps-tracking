-- Separate mail delivery from both HTTP auth and location-data capabilities.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_mailer') THEN
        CREATE ROLE gps_tracking_mailer NOLOGIN NOBYPASSRLS;
    END IF;
END
$$;
ALTER ROLE gps_tracking_mailer NOLOGIN NOBYPASSRLS;
GRANT USAGE ON SCHEMA public TO gps_tracking_mailer;

ALTER TABLE users DROP CONSTRAINT users_status_check;
ALTER TABLE users ADD CONSTRAINT users_status_check
    CHECK (status IN ('pending_activation', 'active', 'disabled'));

ALTER TABLE account_tokens DROP CONSTRAINT account_tokens_purpose_check;
ALTER TABLE account_tokens ADD CONSTRAINT account_tokens_purpose_check
    CHECK (purpose IN ('password_reset', 'activation', 'recovery', 'recovery_email'));

ALTER TABLE email_outbox DROP CONSTRAINT email_outbox_template_check;
ALTER TABLE email_outbox ADD CONSTRAINT email_outbox_template_check
    CHECK (template IN ('invitation', 'password_reset', 'account_activation', 'account_recovery', 'security_notice'));
ALTER TABLE email_outbox ADD COLUMN last_error_code TEXT
    CHECK (last_error_code IS NULL OR last_error_code IN ('smtp_unavailable', 'invalid_message', 'expired', 'attempts_exhausted'));

CREATE TABLE operator_recovery_cases (
    case_id TEXT PRIMARY KEY CHECK (char_length(case_id) BETWEEN 1 AND 128),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    operator_ref TEXT NOT NULL CHECK (char_length(operator_ref) BETWEEN 1 AND 160),
    reason TEXT NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 10 AND 1000),
    recovery_token_id UUID NOT NULL UNIQUE REFERENCES account_tokens(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE operator_recovery_cases OWNER TO gps_tracking_schema_owner;
ALTER TABLE operator_recovery_cases ENABLE ROW LEVEL SECURITY;
ALTER TABLE operator_recovery_cases FORCE ROW LEVEL SECURITY;
CREATE POLICY gps_tracking_auth_recovery_case_insert ON operator_recovery_cases
    FOR INSERT TO gps_tracking_auth WITH CHECK (TRUE);
GRANT INSERT ON operator_recovery_cases TO gps_tracking_auth;

CREATE FUNCTION gps_tracking_platform_admin_exists()
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT EXISTS (SELECT 1 FROM public.users WHERE platform_admin)
$$;
ALTER FUNCTION gps_tracking_platform_admin_exists() OWNER TO gps_tracking_auth_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_platform_admin_exists() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_platform_admin_exists() TO gps_tracking_auth;

CREATE POLICY gps_tracking_mailer_outbox_all ON email_outbox
    FOR ALL TO gps_tracking_mailer USING (TRUE) WITH CHECK (TRUE);
GRANT SELECT (id, recipient, template, encrypted_payload, status, attempts, next_attempt_at, created_at, retention_expires_at)
    ON email_outbox TO gps_tracking_mailer;
GRANT UPDATE (status, attempts, next_attempt_at, sent_at, last_error_code)
    ON email_outbox TO gps_tracking_mailer;
GRANT DELETE ON email_outbox TO gps_tracking_mailer;
