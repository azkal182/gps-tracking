-- Organization metadata and invitation operations use a narrowly scoped
-- platform capability. It can invalidate user sessions but cannot read location.
GRANT UPDATE (status, updated_at) ON organizations TO gps_tracking_platform_provisioner;
GRANT SELECT (id, tenant_id, user_id, role, status) ON memberships
    TO gps_tracking_platform_provisioner;
GRANT SELECT (id, user_id, revoked_at), UPDATE (revoked_at) ON auth_sessions
    TO gps_tracking_platform_provisioner;
GRANT UPDATE (token_hash, status, expires_at, revoked_at) ON invitations
    TO gps_tracking_platform_provisioner;

-- Tenant-scoped auth mutations use the existing FORCE-RLS boundary.
GRANT SELECT ON organizations TO gps_tracking_auth;
GRANT UPDATE (name, updated_at) ON organizations TO gps_tracking_auth;
GRANT SELECT (id, tenant_id, email_canonical, role, status, expires_at, consumed_at, revoked_at, created_at),
    INSERT ON invitations TO gps_tracking_auth;
GRANT UPDATE (token_hash, status, expires_at, consumed_at, revoked_at) ON invitations
    TO gps_tracking_auth;
GRANT INSERT ON memberships TO gps_tracking_auth;
GRANT INSERT ON email_outbox TO gps_tracking_auth;
GRANT INSERT ON audit_events TO gps_tracking_auth;
GRANT DELETE ON email_outbox TO gps_tracking_auth, gps_tracking_platform_provisioner;
GRANT SELECT (tenant_id, recipient, template) ON email_outbox
    TO gps_tracking_auth, gps_tracking_platform_provisioner;

-- This owner exposes only the signed-in user's organization metadata and the
-- minimal invitation data needed to complete an email-verified acceptance.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_org_policy_owner') THEN
        CREATE ROLE gps_tracking_org_policy_owner NOLOGIN BYPASSRLS;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_invitation_owner') THEN
        CREATE ROLE gps_tracking_invitation_owner NOLOGIN BYPASSRLS;
    END IF;
END
$$;
ALTER ROLE gps_tracking_org_policy_owner NOLOGIN BYPASSRLS;
ALTER ROLE gps_tracking_invitation_owner NOLOGIN BYPASSRLS;
GRANT USAGE ON SCHEMA public TO gps_tracking_org_policy_owner, gps_tracking_invitation_owner;

GRANT SELECT (tenant_id, user_id, role, status) ON memberships TO gps_tracking_org_policy_owner;
GRANT SELECT (id, name, status, created_at, updated_at) ON organizations TO gps_tracking_org_policy_owner;
GRANT SELECT (id, status) ON users TO gps_tracking_org_policy_owner;
GRANT SELECT (user_id, enabled_at) ON mfa_factors TO gps_tracking_org_policy_owner;
CREATE FUNCTION gps_tracking_list_user_organizations(p_user_id UUID)
RETURNS TABLE (id UUID, tenant_id UUID, name TEXT, status TEXT, created_at TIMESTAMPTZ, updated_at TIMESTAMPTZ)
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT o.id, o.id, o.name, o.status, o.created_at, o.updated_at
      FROM public.memberships m JOIN public.organizations o ON o.id=m.tenant_id
     WHERE m.user_id=p_user_id AND m.status='active'
     ORDER BY o.created_at DESC, o.id DESC
$$;
ALTER FUNCTION gps_tracking_list_user_organizations(UUID) OWNER TO gps_tracking_org_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_list_user_organizations(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_list_user_organizations(UUID) TO gps_tracking_auth;

GRANT SELECT (id, tenant_id, email_canonical, role, status, token_hash, expires_at, consumed_at, revoked_at, created_at),
    UPDATE (token_hash, status, expires_at, consumed_at, revoked_at) ON invitations TO gps_tracking_invitation_owner;
GRANT SELECT (id, status) ON organizations TO gps_tracking_invitation_owner;
GRANT SELECT (id, email_canonical, status) ON users TO gps_tracking_invitation_owner;
GRANT UPDATE (status) ON users TO gps_tracking_invitation_owner;
GRANT SELECT (user_id, enabled_at) ON mfa_factors TO gps_tracking_invitation_owner;
GRANT SELECT (tenant_id, user_id) ON memberships TO gps_tracking_invitation_owner;
GRANT INSERT ON memberships TO gps_tracking_invitation_owner;

CREATE FUNCTION gps_tracking_invitation_identity(p_token_hash BYTEA)
RETURNS TABLE (id UUID, tenant_id UUID, email_canonical TEXT, role TEXT, status TEXT, expires_at TIMESTAMPTZ)
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT i.id, i.tenant_id, i.email_canonical, i.role, i.status, i.expires_at
      FROM public.invitations i
     WHERE i.token_hash=p_token_hash AND octet_length(p_token_hash)=32
       AND i.status='pending' AND i.expires_at>statement_timestamp()
       AND i.revoked_at IS NULL AND i.consumed_at IS NULL
       AND EXISTS (SELECT 1 FROM public.organizations o WHERE o.id=i.tenant_id AND o.status='active')
     LIMIT 1
$$;
ALTER FUNCTION gps_tracking_invitation_identity(BYTEA) OWNER TO gps_tracking_invitation_owner;
REVOKE ALL ON FUNCTION gps_tracking_invitation_identity(BYTEA) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_invitation_identity(BYTEA) TO gps_tracking_auth;

CREATE FUNCTION gps_tracking_accept_invitation(p_token_hash BYTEA, p_user_id UUID)
RETURNS TABLE (tenant_id UUID, membership_id UUID, role TEXT, membership_status TEXT)
LANGUAGE PLPGSQL SECURITY DEFINER SET search_path = pg_catalog, public SET row_security = off
AS $$
DECLARE
    invite public.invitations%ROWTYPE;
    account_email TEXT;
    account_status TEXT;
    has_mfa BOOLEAN;
    new_membership_id UUID := gen_random_uuid();
    new_status TEXT;
BEGIN
    SELECT i.id,i.tenant_id,i.email_canonical,i.role,i.token_hash,i.status,i.expires_at,
           i.consumed_at,i.revoked_at,i.created_at
      INTO invite FROM public.invitations i
     WHERE token_hash=p_token_hash AND octet_length(p_token_hash)=32
     FOR UPDATE;
    IF NOT FOUND OR invite.status <> 'pending' OR invite.expires_at <= statement_timestamp()
       OR invite.revoked_at IS NOT NULL OR invite.consumed_at IS NOT NULL
       OR NOT EXISTS (SELECT 1 FROM public.organizations o WHERE o.id=invite.tenant_id AND o.status='active') THEN
        RETURN;
    END IF;
    SELECT email_canonical,status INTO account_email,account_status
      FROM public.users WHERE id=p_user_id FOR UPDATE;
    IF NOT FOUND OR account_email <> invite.email_canonical OR account_status <> 'active' THEN
        RETURN;
    END IF;
    IF EXISTS (SELECT 1 FROM public.memberships AS m WHERE m.tenant_id=invite.tenant_id AND m.user_id=p_user_id) THEN
        RETURN;
    END IF;
    SELECT EXISTS (SELECT 1 FROM public.mfa_factors WHERE user_id=p_user_id AND enabled_at IS NOT NULL)
      INTO has_mfa;
    new_status := CASE WHEN invite.role='organization_admin' AND NOT has_mfa THEN 'pending_mfa' ELSE 'active' END;
    INSERT INTO public.memberships(id,tenant_id,user_id,role,status)
      VALUES(new_membership_id,invite.tenant_id,p_user_id,invite.role,new_status);
    UPDATE public.invitations SET status='accepted',consumed_at=clock_timestamp()
     WHERE id=invite.id;
    RETURN QUERY SELECT invite.tenant_id,new_membership_id,invite.role,new_status;
END
$$;
ALTER FUNCTION gps_tracking_accept_invitation(BYTEA, UUID) OWNER TO gps_tracking_invitation_owner;
REVOKE ALL ON FUNCTION gps_tracking_accept_invitation(BYTEA, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_accept_invitation(BYTEA, UUID) TO gps_tracking_auth;

GRANT SELECT (tenant_id, user_id, role, status) ON memberships TO gps_tracking_platform_provisioner;
GRANT SELECT (user_id, enabled_at) ON mfa_factors TO gps_tracking_invitation_owner;
CREATE FUNCTION gps_tracking_organization_has_active_admin(p_tenant_id UUID)
RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT EXISTS (
      SELECT 1 FROM public.memberships m
      JOIN public.users u ON u.id=m.user_id
      JOIN public.mfa_factors f ON f.user_id=u.id AND f.enabled_at IS NOT NULL
      WHERE m.tenant_id=p_tenant_id AND m.role='organization_admin'
        AND m.status='active' AND u.status='active'
    )
$$;
ALTER FUNCTION gps_tracking_organization_has_active_admin(UUID) OWNER TO gps_tracking_org_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_organization_has_active_admin(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_organization_has_active_admin(UUID)
    TO gps_tracking_auth, gps_tracking_platform_provisioner;
