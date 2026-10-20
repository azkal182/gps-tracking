-- Explicit, tenant-bound object grants. Organization-scoped create grants use
-- resource_type='organization' and the organization UUID as resource_id.
CREATE TABLE object_grants (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    membership_id UUID NOT NULL,
    resource_type TEXT NOT NULL CHECK (resource_type IN ('organization', 'target', 'device')),
    target_id UUID,
    device_id UUID,
    permission TEXT NOT NULL CHECK (permission IN ('targets:create', 'devices:create', 'read', 'write', 'credentials:write')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (tenant_id, membership_id) REFERENCES memberships(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, target_id) REFERENCES targets(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, device_id) REFERENCES devices(tenant_id, id) ON DELETE CASCADE,
    CHECK (
        (resource_type = 'organization' AND target_id IS NULL AND device_id IS NULL
            AND permission IN ('targets:create', 'devices:create')) OR
        (resource_type = 'target' AND target_id IS NOT NULL AND device_id IS NULL
            AND permission IN ('read', 'write')) OR
        (resource_type = 'device' AND target_id IS NULL AND device_id IS NOT NULL
            AND permission IN ('read', 'write', 'credentials:write'))
    ),
    UNIQUE NULLS NOT DISTINCT (tenant_id, membership_id, resource_type, target_id, device_id, permission)
);
CREATE INDEX object_grants_membership_idx ON object_grants(tenant_id, membership_id, resource_type, target_id, device_id);
CREATE INDEX object_grants_resource_idx ON object_grants(tenant_id, resource_type, target_id, device_id, permission);
ALTER TABLE object_grants OWNER TO gps_tracking_schema_owner;
ALTER TABLE object_grants ENABLE ROW LEVEL SECURITY;
ALTER TABLE object_grants FORCE ROW LEVEL SECURITY;
CREATE POLICY gps_tracking_tenant_isolation ON object_grants
    FOR ALL TO gps_tracking_app, gps_tracking_schema_owner, gps_tracking_auth
    USING (tenant_id = public.gps_tracking_tenant_id())
    WITH CHECK (tenant_id = public.gps_tracking_tenant_id());

GRANT SELECT ON object_grants TO gps_tracking_app;
GRANT SELECT (tenant_id, membership_id, resource_type, device_id, permission)
    ON object_grants TO gps_tracking_auth_policy_owner;
GRANT SELECT (id, status) ON organizations TO gps_tracking_auth_policy_owner;

-- This role can answer authorization booleans and validate object identity,
-- but cannot read tracking rows or metadata payloads.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_grant_policy_owner') THEN
        CREATE ROLE gps_tracking_grant_policy_owner NOLOGIN BYPASSRLS;
    END IF;
END
$$;
ALTER ROLE gps_tracking_grant_policy_owner NOLOGIN BYPASSRLS;
GRANT USAGE ON SCHEMA public TO gps_tracking_grant_policy_owner;
GRANT SELECT (id, tenant_id, user_id, role, status) ON memberships TO gps_tracking_grant_policy_owner;
GRANT SELECT (id, status) ON organizations TO gps_tracking_grant_policy_owner;
GRANT SELECT (id, status) ON users TO gps_tracking_grant_policy_owner;
GRANT SELECT (tenant_id, membership_id, resource_type, target_id, device_id, permission)
    ON object_grants TO gps_tracking_grant_policy_owner;
GRANT SELECT (id, tenant_id) ON targets, devices TO gps_tracking_grant_policy_owner;
GRANT SELECT (id, tenant_id, owner_user_id, scopes, expires_at, revoked_at)
    ON application_tokens TO gps_tracking_grant_policy_owner;
GRANT SELECT (tenant_id, application_token_id, device_id) ON application_token_devices
    TO gps_tracking_grant_policy_owner;
GRANT SELECT (tenant_id, application_token_id, target_id) ON application_token_targets
    TO gps_tracking_grant_policy_owner;

-- The deferred allowlist invariant also fires when an admin revokes a token.
-- Run the metadata-only check as the narrowly privileged policy owner so the
-- auth principal does not need direct application-token table access.
ALTER FUNCTION assert_application_token_has_allowlist() SECURITY DEFINER;
ALTER FUNCTION assert_application_token_has_allowlist()
    SET search_path = pg_catalog, public;
ALTER FUNCTION assert_application_token_has_allowlist()
    OWNER TO gps_tracking_grant_policy_owner;

CREATE FUNCTION gps_tracking_member_has_permission(
    p_tenant_id UUID, p_user_id UUID, p_resource_type TEXT, p_resource_id UUID, p_permission TEXT
) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.memberships m
        JOIN public.organizations o ON o.id=m.tenant_id
        JOIN public.users u ON u.id=m.user_id AND u.status='active'
        WHERE m.tenant_id=p_tenant_id AND m.user_id=p_user_id
          AND m.status='active' AND o.status='active'
          AND CASE WHEN m.role='organization_admin' THEN TRUE
                   WHEN m.role='viewer' THEN p_permission='read' AND EXISTS (
                       SELECT 1 FROM public.object_grants g WHERE g.tenant_id=m.tenant_id
                         AND g.membership_id=m.id AND g.resource_type=p_resource_type
                         AND COALESCE(g.target_id,g.device_id,m.tenant_id)=p_resource_id
                         AND g.permission IN ('read','write','credentials:write'))
                   ELSE EXISTS (
                       SELECT 1 FROM public.object_grants g WHERE g.tenant_id=m.tenant_id
                         AND g.membership_id=m.id AND g.resource_type=p_resource_type
                         AND COALESCE(g.target_id,g.device_id,m.tenant_id)=p_resource_id
                         AND (g.permission=p_permission OR (p_permission='read' AND g.permission IN ('write','credentials:write'))))
              END
    )
$$;
ALTER FUNCTION gps_tracking_member_has_permission(UUID, UUID, TEXT, UUID, TEXT)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_member_has_permission(UUID, UUID, TEXT, UUID, TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_member_has_permission(UUID, UUID, TEXT, UUID, TEXT)
    TO gps_tracking_auth, gps_tracking_app;

-- Integration access intersects token scopes and allowlist with the owner's
-- live membership/grants on every request. Token digests and location are not read.
CREATE FUNCTION gps_tracking_application_token_owner_has_permission(
    p_token_id UUID,p_tenant UUID,p_resource_type TEXT,p_resource_id UUID,p_permission TEXT
) RETURNS BOOLEAN LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.application_tokens t
        WHERE t.id=p_token_id AND t.tenant_id=p_tenant AND t.revoked_at IS NULL
          AND t.expires_at>statement_timestamp()
          AND CASE
            WHEN p_resource_type='target' AND p_permission='read' THEN t.scopes && ARRAY['targets:read','targets:write']
            WHEN p_resource_type='target' AND p_permission='write' THEN t.scopes @> ARRAY['targets:write']
            WHEN p_resource_type='device' AND p_permission='read' THEN t.scopes && ARRAY['devices:read','devices:write']
            WHEN p_resource_type='device' AND p_permission='write' THEN t.scopes @> ARRAY['devices:write']
            ELSE FALSE END
          AND (CASE WHEN p_resource_type='target' THEN EXISTS (
                SELECT 1 FROM public.application_token_targets a WHERE a.tenant_id=p_tenant
                  AND a.application_token_id=t.id AND a.target_id=p_resource_id)
              WHEN p_resource_type='device' THEN EXISTS (
                SELECT 1 FROM public.application_token_devices a WHERE a.tenant_id=p_tenant
                  AND a.application_token_id=t.id AND a.device_id=p_resource_id)
              ELSE FALSE END)
          AND public.gps_tracking_member_has_permission(p_tenant,t.owner_user_id,p_resource_type,p_resource_id,p_permission)
    )
$$;
ALTER FUNCTION gps_tracking_application_token_owner_has_permission(UUID,UUID,TEXT,UUID,TEXT)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_application_token_owner_has_permission(UUID,UUID,TEXT,UUID,TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_application_token_owner_has_permission(UUID,UUID,TEXT,UUID,TEXT)
    TO gps_tracking_auth, gps_tracking_app;

CREATE FUNCTION gps_tracking_tenant_resource_exists(p_tenant_id UUID, p_resource_type TEXT, p_resource_id UUID)
RETURNS BOOLEAN LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT CASE p_resource_type
      WHEN 'organization' THEN p_resource_id=p_tenant_id AND EXISTS (
          SELECT 1 FROM public.organizations WHERE id=p_tenant_id AND status='active')
      WHEN 'target' THEN EXISTS (SELECT 1 FROM public.targets WHERE tenant_id=p_tenant_id AND id=p_resource_id)
      WHEN 'device' THEN EXISTS (SELECT 1 FROM public.devices WHERE tenant_id=p_tenant_id AND id=p_resource_id)
      ELSE FALSE END
$$;
ALTER FUNCTION gps_tracking_tenant_resource_exists(UUID, TEXT, UUID)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_tenant_resource_exists(UUID, TEXT, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_tenant_resource_exists(UUID, TEXT, UUID) TO gps_tracking_auth;

-- Membership and grant mutations are exposed through narrow functions so the
-- auth principal cannot promote pending-MFA members with direct table UPDATE.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_membership_owner') THEN
        CREATE ROLE gps_tracking_membership_owner NOLOGIN BYPASSRLS;
    END IF;
END
$$;
ALTER ROLE gps_tracking_membership_owner NOLOGIN BYPASSRLS;
GRANT USAGE ON SCHEMA public TO gps_tracking_membership_owner;
GRANT SELECT (id, name, status) ON organizations TO gps_tracking_membership_owner;
GRANT UPDATE (name) ON organizations TO gps_tracking_membership_owner;
GRANT SELECT (id, tenant_id, user_id, role, status, created_at, updated_at),
    UPDATE (role, status, updated_at) ON memberships TO gps_tracking_membership_owner;
GRANT SELECT (id, email_canonical, display_name, status, auth_version) ON users TO gps_tracking_membership_owner;
GRANT SELECT (user_id, enabled_at) ON mfa_factors TO gps_tracking_membership_owner;
GRANT SELECT (id, user_id, reauthenticated_at, revoked_at, auth_version, expires_at, last_interactive_at), UPDATE (revoked_at)
    ON auth_sessions TO gps_tracking_membership_owner;
GRANT SELECT (tenant_id, membership_id, resource_type, target_id, device_id, permission),
    INSERT, DELETE ON object_grants TO gps_tracking_membership_owner;
GRANT SELECT (id, tenant_id, owner_user_id, revoked_at), UPDATE (revoked_at)
    ON application_tokens TO gps_tracking_membership_owner;
-- Deferred allowlist constraint triggers run with the SECURITY DEFINER function
-- owner when membership changes revoke an integration token.
GRANT SELECT (tenant_id, application_token_id) ON application_token_devices, application_token_targets
    TO gps_tracking_membership_owner;
GRANT INSERT ON audit_events TO gps_tracking_membership_owner;
GRANT EXECUTE ON FUNCTION gps_tracking_tenant_resource_exists(UUID, TEXT, UUID)
    TO gps_tracking_membership_owner;

CREATE FUNCTION gps_tracking_update_membership(
    p_tenant UUID, p_actor UUID, p_session UUID, p_target UUID,
    p_role TEXT, p_status TEXT, p_remove BOOLEAN
) RETURNS TABLE (id UUID, tenant_id UUID, user_id UUID, role TEXT, status TEXT,
                 created_at TIMESTAMPTZ, updated_at TIMESTAMPTZ,
                 email_canonical TEXT, display_name TEXT)
LANGUAGE PLPGSQL SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
DECLARE
    current_member public.memberships%ROWTYPE;
    actor_allowed BOOLEAN;
    actor_recent BOOLEAN;
    target_has_mfa BOOLEAN;
    target_user_status TEXT;
    next_role TEXT;
    next_status TEXT;
    current_is_admin BOOLEAN;
    next_is_admin BOOLEAN;
    active_admins BIGINT;
BEGIN
    PERFORM 1 FROM public.organizations o WHERE o.id=p_tenant AND o.status='active' FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'organization unavailable' USING ERRCODE='42501'; END IF;
    SELECT EXISTS(SELECT 1 FROM public.memberships m WHERE m.tenant_id=p_tenant AND m.user_id=p_actor
                  AND m.role='organization_admin' AND m.status='active')
      INTO actor_allowed;
    SELECT EXISTS(SELECT 1 FROM public.auth_sessions s JOIN public.users u ON u.id=s.user_id
                  WHERE s.id=p_session AND s.user_id=p_actor AND s.revoked_at IS NULL
                    AND s.reauthenticated_at>statement_timestamp()-interval '5 minutes'
                    AND s.expires_at>statement_timestamp() AND s.last_interactive_at>statement_timestamp()-interval '30 minutes'
                    AND s.auth_version=u.auth_version AND u.status='active' AND EXISTS(SELECT 1 FROM public.mfa_factors f
                      WHERE f.user_id=u.id AND f.enabled_at IS NOT NULL)) INTO actor_recent;
    IF NOT actor_allowed OR NOT actor_recent THEN
        RAISE EXCEPTION 'recent organization admin authentication required' USING ERRCODE='42501';
    END IF;
    SELECT m.* INTO current_member FROM public.memberships m
      WHERE m.tenant_id=p_tenant AND m.user_id=p_target FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'membership not found' USING ERRCODE='P0002'; END IF;
    IF p_role IS NOT NULL AND p_role NOT IN ('organization_admin','operator','viewer') THEN
        RAISE EXCEPTION 'invalid membership role' USING ERRCODE='22023';
    END IF;
    IF p_status IS NOT NULL AND p_status NOT IN ('active','suspended') THEN
        RAISE EXCEPTION 'invalid membership status' USING ERRCODE='22023';
    END IF;
    IF NOT p_remove AND p_role IS NULL AND p_status IS NULL THEN
        RAISE EXCEPTION 'membership change is empty' USING ERRCODE='22023';
    END IF;
    SELECT EXISTS(SELECT 1 FROM public.mfa_factors f WHERE f.user_id=p_target AND f.enabled_at IS NOT NULL),
           u.status INTO target_has_mfa,target_user_status
      FROM public.users u WHERE u.id=p_target;
    next_role := COALESCE(p_role,current_member.role);
    next_status := CASE WHEN p_remove THEN 'suspended' ELSE COALESCE(p_status,current_member.status) END;
    IF NOT p_remove AND next_role='organization_admin' AND next_status='active' AND NOT target_has_mfa THEN
        next_status := 'pending_mfa';
    END IF;
    current_is_admin := current_member.role='organization_admin' AND current_member.status='active'
                        AND target_has_mfa AND target_user_status='active';
    next_is_admin := next_role='organization_admin' AND next_status='active'
                     AND target_has_mfa AND target_user_status='active';
    IF current_is_admin AND NOT next_is_admin THEN
        SELECT count(*) INTO active_admins FROM public.memberships m
          JOIN public.users u ON u.id=m.user_id AND u.status='active'
          JOIN public.mfa_factors f ON f.user_id=u.id AND f.enabled_at IS NOT NULL
          WHERE m.tenant_id=p_tenant AND m.role='organization_admin' AND m.status='active';
        IF active_admins<=1 THEN RAISE EXCEPTION 'last active administrator' USING ERRCODE='23514'; END IF;
    END IF;
    UPDATE public.memberships m SET role=next_role,status=next_status,updated_at=clock_timestamp()
      WHERE m.id=current_member.id;
    IF p_remove THEN
        DELETE FROM public.object_grants g WHERE g.tenant_id=p_tenant AND g.membership_id=current_member.id;
    END IF;
    IF p_remove OR next_status<>'active' THEN
        UPDATE public.application_tokens t SET revoked_at=clock_timestamp()
          WHERE t.tenant_id=p_tenant AND t.owner_user_id=p_target AND t.revoked_at IS NULL;
    END IF;
    UPDATE public.auth_sessions s SET revoked_at=clock_timestamp()
      WHERE s.user_id=p_target AND s.revoked_at IS NULL;
    INSERT INTO public.audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id)
      VALUES(p_tenant,'user:'||p_actor::text,CASE WHEN p_remove THEN 'membership.removed' ELSE 'membership.updated' END,
             'membership',current_member.id,'success',gen_random_uuid()::text);
    RETURN QUERY SELECT m.id,m.tenant_id,m.user_id,m.role,m.status,m.created_at,m.updated_at,u.email_canonical,u.display_name
      FROM public.memberships m JOIN public.users u ON u.id=m.user_id WHERE m.id=current_member.id;
END
$$;
ALTER FUNCTION gps_tracking_update_membership(UUID,UUID,UUID,UUID,TEXT,TEXT,BOOLEAN)
    OWNER TO gps_tracking_membership_owner;
REVOKE ALL ON FUNCTION gps_tracking_update_membership(UUID,UUID,UUID,UUID,TEXT,TEXT,BOOLEAN) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_update_membership(UUID,UUID,UUID,UUID,TEXT,TEXT,BOOLEAN)
    TO gps_tracking_auth;

CREATE FUNCTION gps_tracking_list_member_grants(p_tenant UUID,p_actor UUID,p_session UUID,p_target UUID)
RETURNS TABLE(resource_type TEXT,resource_id UUID,permission TEXT)
LANGUAGE PLPGSQL SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM public.organizations o JOIN public.memberships a
        ON a.tenant_id=o.id AND a.user_id=p_actor AND a.role='organization_admin' AND a.status='active'
        JOIN public.memberships m ON m.tenant_id=o.id AND m.user_id=p_target
        JOIN public.auth_sessions s ON s.user_id=p_actor AND s.id=p_session
        JOIN public.users u ON u.id=p_actor AND u.status='active' AND s.auth_version=u.auth_version
        WHERE o.id=p_tenant AND o.status='active' AND s.revoked_at IS NULL
          AND s.expires_at>statement_timestamp() AND s.last_interactive_at>statement_timestamp()-interval '30 minutes') THEN
        RAISE EXCEPTION 'organization admin and membership required' USING ERRCODE='42501';
    END IF;
    RETURN QUERY SELECT g.resource_type,COALESCE(g.target_id,g.device_id,g.tenant_id),g.permission
      FROM public.object_grants g JOIN public.memberships m ON m.id=g.membership_id
      WHERE g.tenant_id=p_tenant AND m.user_id=p_target
      ORDER BY g.resource_type,COALESCE(g.target_id,g.device_id,g.tenant_id),g.permission;
END
$$;
ALTER FUNCTION gps_tracking_list_member_grants(UUID,UUID,UUID,UUID) OWNER TO gps_tracking_membership_owner;
REVOKE ALL ON FUNCTION gps_tracking_list_member_grants(UUID,UUID,UUID,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_list_member_grants(UUID,UUID,UUID,UUID) TO gps_tracking_auth;

CREATE FUNCTION gps_tracking_replace_member_grants(
    p_tenant UUID,p_actor UUID,p_session UUID,p_target UUID,p_grants JSONB
) RETURNS TABLE(resource_type TEXT,resource_id UUID,permission TEXT)
LANGUAGE PLPGSQL SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
DECLARE
    grant_item JSONB;
    grant_type TEXT;
    grant_id UUID;
    grant_permissions JSONB;
    permission_item JSONB;
    permission_name TEXT;
    member_id UUID;
BEGIN
    PERFORM 1 FROM public.organizations o WHERE o.id=p_tenant AND o.status='active' FOR UPDATE;
    IF NOT FOUND OR NOT EXISTS(SELECT 1 FROM public.memberships WHERE tenant_id=p_tenant AND user_id=p_actor
        AND role='organization_admin' AND status='active') OR NOT EXISTS(
        SELECT 1 FROM public.auth_sessions s JOIN public.users u ON u.id=s.user_id
        WHERE s.id=p_session AND s.user_id=p_actor AND s.revoked_at IS NULL
          AND s.reauthenticated_at>statement_timestamp()-interval '5 minutes'
          AND s.expires_at>statement_timestamp() AND s.last_interactive_at>statement_timestamp()-interval '30 minutes'
          AND s.auth_version=u.auth_version AND u.status='active'
          AND EXISTS(SELECT 1 FROM public.mfa_factors f WHERE f.user_id=u.id AND f.enabled_at IS NOT NULL)) THEN
        RAISE EXCEPTION 'recent organization admin authentication required' USING ERRCODE='42501';
    END IF;
    SELECT id INTO member_id FROM public.memberships WHERE tenant_id=p_tenant AND user_id=p_target FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'membership not found' USING ERRCODE='P0002'; END IF;
    IF jsonb_typeof(p_grants)<>'array' OR jsonb_array_length(p_grants)>1000 THEN
        RAISE EXCEPTION 'invalid grant list' USING ERRCODE='22023';
    END IF;
    FOR grant_item IN SELECT value FROM jsonb_array_elements(p_grants) LOOP
        grant_type := grant_item->>'resource_type';
        grant_id := (grant_item->>'resource_id')::uuid;
        grant_permissions := grant_item->'permissions';
        IF jsonb_typeof(grant_permissions)<>'array' OR jsonb_array_length(grant_permissions)=0 THEN
            RAISE EXCEPTION 'invalid grant permissions' USING ERRCODE='22023';
        END IF;
        IF grant_type NOT IN ('organization','target','device') THEN
            RAISE EXCEPTION 'invalid grant resource type' USING ERRCODE='22023';
        END IF;
        IF (grant_type='organization' AND grant_id<>p_tenant) OR
           NOT public.gps_tracking_tenant_resource_exists(p_tenant,grant_type,grant_id) THEN
            RAISE EXCEPTION 'grant resource is outside tenant or missing' USING ERRCODE='P0002';
        END IF;
        FOR permission_item IN SELECT value FROM jsonb_array_elements(grant_permissions) LOOP
            permission_name := permission_item #>> '{}';
            IF NOT ((grant_type='organization' AND permission_name IN ('targets:create','devices:create')) OR
                    (grant_type='target' AND permission_name IN ('read','write')) OR
                    (grant_type='device' AND permission_name IN ('read','write','credentials:write'))) THEN
                RAISE EXCEPTION 'permission not valid for resource type' USING ERRCODE='22023';
            END IF;
        END LOOP;
    END LOOP;
    DELETE FROM public.object_grants WHERE tenant_id=p_tenant AND membership_id=member_id;
    FOR grant_item IN SELECT value FROM jsonb_array_elements(p_grants) LOOP
        grant_type := grant_item->>'resource_type';
        grant_id := (grant_item->>'resource_id')::uuid;
        FOR permission_item IN SELECT value FROM jsonb_array_elements(grant_item->'permissions') LOOP
            permission_name := permission_item #>> '{}';
            INSERT INTO public.object_grants(id,tenant_id,membership_id,resource_type,target_id,device_id,permission)
            VALUES(gen_random_uuid(),p_tenant,member_id,grant_type,
                   CASE WHEN grant_type='target' THEN grant_id END,
                   CASE WHEN grant_type='device' THEN grant_id END,permission_name);
        END LOOP;
    END LOOP;
    UPDATE public.auth_sessions SET revoked_at=clock_timestamp() WHERE user_id=p_target AND revoked_at IS NULL;
    INSERT INTO public.audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id)
      VALUES(p_tenant,'user:'||p_actor::text,'membership.grants_replaced','membership',member_id,'success',gen_random_uuid()::text);
    RETURN QUERY SELECT g.resource_type,COALESCE(g.target_id,g.device_id,g.tenant_id),g.permission
      FROM public.object_grants g WHERE g.tenant_id=p_tenant AND g.membership_id=member_id
      ORDER BY g.resource_type,COALESCE(g.target_id,g.device_id,g.tenant_id),g.permission;
END
$$;
ALTER FUNCTION gps_tracking_replace_member_grants(UUID,UUID,UUID,UUID,JSONB)
    OWNER TO gps_tracking_membership_owner;
REVOKE ALL ON FUNCTION gps_tracking_replace_member_grants(UUID,UUID,UUID,UUID,JSONB) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_replace_member_grants(UUID,UUID,UUID,UUID,JSONB)
    TO gps_tracking_auth;

CREATE FUNCTION gps_tracking_grant_creator_object_access(
    p_tenant UUID,p_user UUID,p_resource_type TEXT,p_resource_id UUID
) RETURNS VOID LANGUAGE PLPGSQL SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
DECLARE
    member_id UUID;
BEGIN
    IF p_resource_type NOT IN ('target','device') OR NOT public.gps_tracking_tenant_resource_exists(p_tenant,p_resource_type,p_resource_id) THEN
        RAISE EXCEPTION 'creator resource unavailable' USING ERRCODE='42501';
    END IF;
    SELECT m.id INTO member_id FROM public.memberships m JOIN public.organizations o ON o.id=m.tenant_id
      WHERE m.tenant_id=p_tenant AND m.user_id=p_user AND m.status='active' AND o.status='active';
    IF NOT FOUND THEN RAISE EXCEPTION 'creator membership unavailable' USING ERRCODE='42501'; END IF;
    INSERT INTO public.object_grants(id,tenant_id,membership_id,resource_type,target_id,device_id,permission)
    VALUES(gen_random_uuid(),p_tenant,member_id,p_resource_type,
           CASE WHEN p_resource_type='target' THEN p_resource_id END,
           CASE WHEN p_resource_type='device' THEN p_resource_id END,'read'),
          (gen_random_uuid(),p_tenant,member_id,p_resource_type,
           CASE WHEN p_resource_type='target' THEN p_resource_id END,
           CASE WHEN p_resource_type='device' THEN p_resource_id END,'write');
END
$$;
ALTER FUNCTION gps_tracking_grant_creator_object_access(UUID,UUID,TEXT,UUID)
    OWNER TO gps_tracking_membership_owner;
REVOKE ALL ON FUNCTION gps_tracking_grant_creator_object_access(UUID,UUID,TEXT,UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_grant_creator_object_access(UUID,UUID,TEXT,UUID)
    TO gps_tracking_auth, gps_tracking_app;

-- Login policy exposes only a boolean. An operator entrusted with device
-- credential management must enroll MFA before logging into management APIs.
CREATE OR REPLACE FUNCTION gps_tracking_user_requires_mfa(p_user_id UUID)
RETURNS BOOLEAN LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.users u
        WHERE u.id=p_user_id AND u.status='active' AND u.platform_admin
    ) OR EXISTS (
        SELECT 1 FROM public.memberships m
        WHERE m.user_id=p_user_id
          AND (m.status='pending_mfa' OR (m.status='active' AND m.role='organization_admin'))
    ) OR EXISTS (
        SELECT 1 FROM public.memberships m
        JOIN public.object_grants g ON g.tenant_id=m.tenant_id AND g.membership_id=m.id
        JOIN public.organizations o ON o.id=m.tenant_id AND o.status='active'
        WHERE m.user_id=p_user_id AND m.status='active' AND m.role='operator'
          AND g.resource_type='device' AND g.permission='credentials:write'
    )
$$;
-- Preserve the narrow policy owner and grants defined in migration 005.
