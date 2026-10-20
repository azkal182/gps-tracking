DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_app') THEN
        CREATE ROLE gps_tracking_app NOLOGIN NOBYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_schema_owner') THEN
        CREATE ROLE gps_tracking_schema_owner NOLOGIN NOBYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_device_lookup_owner') THEN
        CREATE ROLE gps_tracking_device_lookup_owner NOLOGIN BYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_retention_worker') THEN
        CREATE ROLE gps_tracking_retention_worker NOLOGIN NOBYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_platform_provisioner') THEN
        CREATE ROLE gps_tracking_platform_provisioner NOLOGIN BYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_auth') THEN
        CREATE ROLE gps_tracking_auth NOLOGIN NOBYPASSRLS;
    END IF;
END
$$;

ALTER ROLE gps_tracking_app NOLOGIN NOBYPASSRLS;
ALTER ROLE gps_tracking_schema_owner NOLOGIN NOBYPASSRLS;
ALTER ROLE gps_tracking_device_lookup_owner NOLOGIN BYPASSRLS;
ALTER ROLE gps_tracking_retention_worker NOLOGIN NOBYPASSRLS;
ALTER ROLE gps_tracking_platform_provisioner NOLOGIN BYPASSRLS;
ALTER ROLE gps_tracking_auth NOLOGIN NOBYPASSRLS;

GRANT USAGE, CREATE ON SCHEMA public TO gps_tracking_schema_owner;
GRANT USAGE ON SCHEMA public TO gps_tracking_app, gps_tracking_device_lookup_owner,
    gps_tracking_retention_worker, gps_tracking_platform_provisioner, gps_tracking_auth;

CREATE FUNCTION gps_tracking_tenant_id()
RETURNS UUID
LANGUAGE SQL
STABLE
PARALLEL SAFE
AS $$
    SELECT NULLIF(pg_catalog.current_setting('app.tenant_id', TRUE), '')::UUID
$$;
ALTER FUNCTION gps_tracking_tenant_id() OWNER TO gps_tracking_schema_owner;
REVOKE ALL ON FUNCTION gps_tracking_tenant_id() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_tenant_id()
    TO gps_tracking_app, gps_tracking_schema_owner, gps_tracking_retention_worker, gps_tracking_auth;

CREATE FUNCTION lookup_device_credential(p_token_hash BYTEA)
RETURNS TABLE (tenant_id UUID, device_id UUID, ingestion_policy TEXT)
LANGUAGE SQL
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT d.tenant_id, d.id, d.ingestion_policy
    FROM public.device_credentials AS credential
    JOIN public.devices AS d
      ON d.tenant_id = credential.tenant_id
     AND d.id = credential.device_id
    JOIN public.organizations AS organization
      ON organization.id = d.tenant_id
    WHERE credential.token_hash = p_token_hash
      AND credential.revoked_at IS NULL
      AND d.enabled
      AND organization.status = 'active'
      AND pg_catalog.octet_length(p_token_hash) = 32
    LIMIT 1
$$;
ALTER FUNCTION lookup_device_credential(BYTEA) OWNER TO gps_tracking_device_lookup_owner;
REVOKE ALL ON FUNCTION lookup_device_credential(BYTEA) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION lookup_device_credential(BYTEA) TO gps_tracking_app;
GRANT SELECT (tenant_id, device_id, token_hash, revoked_at) ON device_credentials
    TO gps_tracking_device_lookup_owner;
GRANT SELECT (id, tenant_id, ingestion_policy, enabled) ON devices
    TO gps_tracking_device_lookup_owner;
GRANT SELECT (id, status) ON organizations TO gps_tracking_device_lookup_owner;

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'organizations', 'memberships', 'invitations', 'email_outbox',
        'targets', 'devices', 'device_credentials', 'application_tokens',
        'application_token_devices', 'application_token_targets', 'tracking_sessions',
        'positions', 'idempotency_records', 'audit_events'
    ] LOOP
        EXECUTE pg_catalog.format(
            'ALTER TABLE public.%I OWNER TO gps_tracking_schema_owner', table_name
        );
    END LOOP;
END
$$;

CREATE FUNCTION gps_tracking_apply_tenant_rls(p_table REGCLASS, p_tenant_expression TEXT)
RETURNS VOID
LANGUAGE PLPGSQL
AS $$
BEGIN
    EXECUTE pg_catalog.format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', p_table);
    EXECUTE pg_catalog.format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', p_table);
    EXECUTE pg_catalog.format('DROP POLICY IF EXISTS gps_tracking_tenant_isolation ON %s', p_table);
    EXECUTE pg_catalog.format(
        'CREATE POLICY gps_tracking_tenant_isolation ON %s FOR ALL TO gps_tracking_app, gps_tracking_schema_owner, gps_tracking_auth USING ((%s) = public.gps_tracking_tenant_id()) WITH CHECK ((%s) = public.gps_tracking_tenant_id())',
        p_table, p_tenant_expression, p_tenant_expression
    );
END
$$;
ALTER FUNCTION gps_tracking_apply_tenant_rls(REGCLASS, TEXT) OWNER TO gps_tracking_schema_owner;
SELECT gps_tracking_apply_tenant_rls('public.organizations', 'id');

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'memberships', 'invitations', 'email_outbox', 'targets', 'devices',
        'device_credentials', 'application_tokens', 'application_token_devices',
        'application_token_targets', 'tracking_sessions', 'positions',
        'idempotency_records', 'audit_events'
    ] LOOP
        PERFORM gps_tracking_apply_tenant_rls(
            pg_catalog.to_regclass(pg_catalog.format('public.%I', table_name)), 'tenant_id'
        );
    END LOOP;
END
$$;
DROP FUNCTION gps_tracking_apply_tenant_rls(REGCLASS, TEXT);

CREATE POLICY gps_tracking_retention_positions_select ON positions
    FOR SELECT TO gps_tracking_retention_worker
    USING (expires_at <= pg_catalog.statement_timestamp());
CREATE POLICY gps_tracking_retention_positions_delete ON positions
    FOR DELETE TO gps_tracking_retention_worker
    USING (expires_at <= pg_catalog.statement_timestamp());
CREATE POLICY gps_tracking_retention_idempotency_select ON idempotency_records
    FOR SELECT TO gps_tracking_retention_worker
    USING (expires_at <= pg_catalog.statement_timestamp());
CREATE POLICY gps_tracking_retention_idempotency_delete ON idempotency_records
    FOR DELETE TO gps_tracking_retention_worker
    USING (expires_at <= pg_catalog.statement_timestamp());
CREATE POLICY gps_tracking_retention_audit_select ON audit_events
    FOR SELECT TO gps_tracking_retention_worker
    USING (occurred_at < pg_catalog.statement_timestamp() - INTERVAL '365 days');
CREATE POLICY gps_tracking_retention_audit_delete ON audit_events
    FOR DELETE TO gps_tracking_retention_worker
    USING (occurred_at < pg_catalog.statement_timestamp() - INTERVAL '365 days');
CREATE POLICY gps_tracking_auth_global_outbox_insert ON email_outbox
    FOR INSERT TO gps_tracking_auth WITH CHECK (tenant_id IS NULL);
CREATE POLICY gps_tracking_auth_global_audit_insert ON audit_events
    FOR INSERT TO gps_tracking_auth WITH CHECK (tenant_id IS NULL);

GRANT SELECT ON organizations TO gps_tracking_app;
GRANT UPDATE (name, updated_at) ON organizations TO gps_tracking_app;
GRANT SELECT ON memberships TO gps_tracking_app;
GRANT INSERT ON memberships TO gps_tracking_app;
GRANT UPDATE (role, status, updated_at) ON memberships TO gps_tracking_app;
GRANT SELECT (id, tenant_id, email_canonical, role, status, expires_at, consumed_at, revoked_at, created_at)
    ON invitations TO gps_tracking_app;
GRANT INSERT ON invitations TO gps_tracking_app;
GRANT UPDATE (status, expires_at, consumed_at, revoked_at) ON invitations TO gps_tracking_app;
GRANT INSERT ON email_outbox TO gps_tracking_app;
GRANT SELECT, INSERT, UPDATE ON targets, devices TO gps_tracking_app;
GRANT SELECT (id, tenant_id, device_id, created_at, revoked_at) ON device_credentials TO gps_tracking_app;
GRANT INSERT (id, tenant_id, device_id, token_hash, created_at) ON device_credentials TO gps_tracking_app;
GRANT UPDATE (revoked_at) ON device_credentials TO gps_tracking_app;
GRANT SELECT (id, tenant_id, owner_user_id, name, scopes, expires_at, revoked_at, created_at)
    ON application_tokens TO gps_tracking_app;
GRANT INSERT ON application_tokens TO gps_tracking_app;
GRANT UPDATE (revoked_at) ON application_tokens TO gps_tracking_app;
GRANT SELECT, INSERT, DELETE ON application_token_devices, application_token_targets TO gps_tracking_app;
GRANT SELECT, INSERT ON tracking_sessions TO gps_tracking_app;
GRANT UPDATE (ended_at) ON tracking_sessions TO gps_tracking_app;
GRANT SELECT, INSERT ON positions TO gps_tracking_app;
GRANT SELECT, INSERT, DELETE ON idempotency_records TO gps_tracking_app;
GRANT SELECT, INSERT ON audit_events TO gps_tracking_app;

GRANT SELECT (id, expires_at), DELETE ON positions TO gps_tracking_retention_worker;
GRANT SELECT (id, expires_at), DELETE ON idempotency_records TO gps_tracking_retention_worker;
GRANT SELECT (id, occurred_at), DELETE ON audit_events TO gps_tracking_retention_worker;

-- Platform provisioning is intentionally unable to read or mutate tracking data.
GRANT SELECT, INSERT ON organizations TO gps_tracking_platform_provisioner;
GRANT SELECT (id, tenant_id, email_canonical, role, status, expires_at, consumed_at, revoked_at, created_at),
    INSERT ON invitations TO gps_tracking_platform_provisioner;
GRANT INSERT ON email_outbox TO gps_tracking_platform_provisioner;
GRANT INSERT ON audit_events TO gps_tracking_platform_provisioner;

-- The auth capability can access global identity/session material, but has no
-- table grants on tracking data; membership reads still require tenant context.
GRANT SELECT (id, email_canonical, display_name, password_hash, status, platform_admin, auth_version, created_at, updated_at),
    INSERT ON users TO gps_tracking_auth;
GRANT UPDATE (display_name, password_hash, status, auth_version, updated_at) ON users TO gps_tracking_auth;
GRANT SELECT, INSERT, UPDATE, DELETE ON auth_sessions, auth_challenges,
    mfa_factors, recovery_codes, account_tokens TO gps_tracking_auth;
GRANT SELECT ON memberships TO gps_tracking_auth;
GRANT INSERT ON email_outbox, audit_events TO gps_tracking_auth;
