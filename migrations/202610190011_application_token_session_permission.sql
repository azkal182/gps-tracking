-- Session access has a separate bearer scope from target/device management
-- scopes. Still intersect it with the token resource allowlist and the owner's
-- current object grants on every tracking-session request.
CREATE FUNCTION gps_tracking_application_token_owner_has_session_permission(
    p_token_id UUID,
    p_tenant_id UUID,
    p_resource_type TEXT,
    p_resource_id UUID,
    p_permission TEXT
) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM public.application_tokens t
        WHERE t.id = p_token_id
          AND t.tenant_id = p_tenant_id
          AND t.revoked_at IS NULL
          AND t.expires_at > statement_timestamp()
          AND p_resource_type IN ('target', 'device')
          AND p_permission IN ('read', 'write')
          AND CASE WHEN p_permission = 'read'
              THEN t.scopes && ARRAY['sessions:read', 'sessions:write']
              ELSE t.scopes @> ARRAY['sessions:write']
          END
          AND CASE WHEN p_resource_type = 'target' THEN EXISTS (
              SELECT 1 FROM public.application_token_targets a
              WHERE a.tenant_id = p_tenant_id
                AND a.application_token_id = t.id
                AND a.target_id = p_resource_id
          ) ELSE EXISTS (
              SELECT 1 FROM public.application_token_devices a
              WHERE a.tenant_id = p_tenant_id
                AND a.application_token_id = t.id
                AND a.device_id = p_resource_id
          ) END
          AND public.gps_tracking_member_has_permission(
              p_tenant_id, t.owner_user_id, p_resource_type, p_resource_id, p_permission
          )
    )
$$;

ALTER FUNCTION gps_tracking_application_token_owner_has_session_permission(UUID, UUID, TEXT, UUID, TEXT)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_application_token_owner_has_session_permission(UUID, UUID, TEXT, UUID, TEXT)
    FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_application_token_owner_has_session_permission(UUID, UUID, TEXT, UUID, TEXT)
    TO gps_tracking_app;
