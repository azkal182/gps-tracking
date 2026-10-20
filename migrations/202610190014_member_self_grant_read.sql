-- Allow an active member to read only their own explicit grants. Administrators
-- retain the ability to inspect another member within the same active tenant.
CREATE OR REPLACE FUNCTION gps_tracking_list_member_grants(
    p_tenant UUID,
    p_actor UUID,
    p_session UUID,
    p_target UUID
)
RETURNS TABLE(resource_type TEXT, resource_id UUID, permission TEXT)
LANGUAGE PLPGSQL SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM public.organizations o
        JOIN public.memberships actor_membership
          ON actor_membership.tenant_id=o.id
         AND actor_membership.user_id=p_actor
         AND actor_membership.status='active'
        JOIN public.auth_sessions s
          ON s.user_id=p_actor AND s.id=p_session
        JOIN public.users u ON u.id=p_actor AND u.status='active'
        WHERE o.id=p_tenant AND o.status='active'
          AND s.auth_version=u.auth_version
          AND s.revoked_at IS NULL
          AND s.expires_at>statement_timestamp()
          AND s.last_interactive_at>statement_timestamp()-interval '30 minutes'
    ) OR NOT EXISTS (
        SELECT 1 FROM public.memberships target_membership
        WHERE target_membership.tenant_id=p_tenant
          AND target_membership.user_id=p_target
    ) OR (
        p_actor <> p_target AND NOT EXISTS (
            SELECT 1
            FROM public.memberships admin_membership
            WHERE admin_membership.tenant_id=p_tenant
              AND admin_membership.user_id=p_actor
              AND admin_membership.role='organization_admin'
              AND admin_membership.status='active'
        )
    ) THEN
        RAISE EXCEPTION 'active tenant membership and self or administrator access required'
            USING ERRCODE='42501';
    END IF;

    RETURN QUERY
    SELECT g.resource_type, COALESCE(g.target_id, g.device_id, g.tenant_id), g.permission
    FROM public.object_grants g
    JOIN public.memberships m ON m.id=g.membership_id
    WHERE g.tenant_id=p_tenant AND m.user_id=p_target
    ORDER BY g.resource_type, COALESCE(g.target_id, g.device_id, g.tenant_id), g.permission;
END
$$;
