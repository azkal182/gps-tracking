-- Expose only the signed-in user's membership metadata to the auth capability.
-- The endpoint obtains the principal from its validated cookie session and
-- never trusts a caller-supplied user ID.
GRANT SELECT (id, tenant_id, user_id, role, status, created_at, updated_at)
    ON memberships TO gps_tracking_org_policy_owner;

CREATE FUNCTION gps_tracking_list_user_memberships(p_user_id UUID)
RETURNS TABLE (
    id UUID,
    tenant_id UUID,
    user_id UUID,
    role TEXT,
    status TEXT,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)
LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT m.id, m.tenant_id, m.user_id, m.role::TEXT, m.status::TEXT,
           m.created_at, m.updated_at
      FROM public.memberships m
     WHERE m.user_id = p_user_id
     ORDER BY m.created_at DESC, m.id DESC
$$;
ALTER FUNCTION gps_tracking_list_user_memberships(UUID)
    OWNER TO gps_tracking_org_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_list_user_memberships(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_list_user_memberships(UUID)
    TO gps_tracking_auth;
