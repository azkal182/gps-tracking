-- Organization-admin permissions still apply only to resources owned by the
-- requested tenant. RLS remains the primary isolation boundary; this also
-- protects callers if a runtime connection is accidentally over-privileged.
CREATE OR REPLACE FUNCTION gps_tracking_member_has_permission(
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
          AND CASE p_resource_type
                WHEN 'organization' THEN p_resource_id=p_tenant_id
                WHEN 'target' THEN EXISTS (
                    SELECT 1 FROM public.targets t
                    WHERE t.tenant_id=p_tenant_id AND t.id=p_resource_id
                )
                WHEN 'device' THEN EXISTS (
                    SELECT 1 FROM public.devices d
                    WHERE d.tenant_id=p_tenant_id AND d.id=p_resource_id
                )
                ELSE FALSE
              END
          AND CASE WHEN m.role='organization_admin' THEN TRUE
                   WHEN m.role='viewer' THEN p_permission='read' AND EXISTS (
                       SELECT 1 FROM public.object_grants g WHERE g.tenant_id=m.tenant_id
                         AND g.membership_id=m.id AND g.resource_type=p_resource_type
                         AND COALESCE(g.target_id,g.device_id,g.tenant_id)=p_resource_id
                         AND g.permission IN ('read','write','credentials:write'))
                   ELSE EXISTS (
                       SELECT 1 FROM public.object_grants g WHERE g.tenant_id=m.tenant_id
                         AND g.membership_id=m.id AND g.resource_type=p_resource_type
                         AND COALESCE(g.target_id,g.device_id,g.tenant_id)=p_resource_id
                         AND (g.permission=p_permission OR (p_permission='read' AND g.permission IN ('write','credentials:write'))))
              END
    )
$$;
ALTER FUNCTION gps_tracking_member_has_permission(UUID, UUID, TEXT, UUID, TEXT)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_member_has_permission(UUID, UUID, TEXT, UUID, TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_member_has_permission(UUID, UUID, TEXT, UUID, TEXT)
    TO gps_tracking_auth, gps_tracking_app;
