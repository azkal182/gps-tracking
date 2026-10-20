-- Narrow retention capability. The retention principal can see only rows eligible
-- for deletion and can never read location payloads or mutate tracking objects.
CREATE POLICY gps_tracking_retention_outbox_select ON email_outbox
    FOR SELECT TO gps_tracking_retention_worker
    USING (retention_expires_at <= pg_catalog.statement_timestamp());
CREATE POLICY gps_tracking_retention_outbox_delete ON email_outbox
    FOR DELETE TO gps_tracking_retention_worker
    USING (retention_expires_at <= pg_catalog.statement_timestamp());
GRANT SELECT (id, retention_expires_at), DELETE ON email_outbox TO gps_tracking_retention_worker;

-- Global auth tables are not RLS protected; grants expose no secret/hash columns.
GRANT SELECT (id, expires_at, revoked_at), DELETE ON auth_sessions TO gps_tracking_retention_worker;
GRANT SELECT (id, expires_at, consumed_at), DELETE ON auth_challenges TO gps_tracking_retention_worker;
GRANT SELECT (id, expires_at, consumed_at, revoked_at), DELETE ON account_tokens TO gps_tracking_retention_worker;
GRANT SELECT (id, created_at, consumed_at), DELETE ON recovery_codes TO gps_tracking_retention_worker;
