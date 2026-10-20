-- Login must know whether an account has a mandatory MFA policy before a
-- tenant context exists. Expose only this boolean through a narrow capability.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_auth_policy_owner') THEN
        CREATE ROLE gps_tracking_auth_policy_owner NOLOGIN BYPASSRLS;
    END IF;
END
$$;
ALTER ROLE gps_tracking_auth_policy_owner NOLOGIN BYPASSRLS;
GRANT USAGE ON SCHEMA public TO gps_tracking_auth_policy_owner;
GRANT SELECT (id, tenant_id, user_id, role, status) ON memberships TO gps_tracking_auth_policy_owner;
GRANT SELECT (id, platform_admin, status) ON users TO gps_tracking_auth_policy_owner;

CREATE FUNCTION gps_tracking_user_requires_mfa(p_user_id UUID)
RETURNS BOOLEAN
LANGUAGE SQL
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.users u
        WHERE u.id = p_user_id AND u.status = 'active' AND u.platform_admin
    ) OR EXISTS (
        SELECT 1 FROM public.memberships m
        WHERE m.user_id = p_user_id
          AND (m.status = 'pending_mfa'
               OR (m.status = 'active' AND m.role = 'organization_admin'))
    )
$$;
ALTER FUNCTION gps_tracking_user_requires_mfa(UUID) OWNER TO gps_tracking_auth_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_user_requires_mfa(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_user_requires_mfa(UUID) TO gps_tracking_auth;
