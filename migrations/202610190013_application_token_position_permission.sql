-- Position reads require their own scope in addition to object allowlists and
-- the token owner's current membership/grant. Session scopes are not equivalent.
CREATE FUNCTION gps_tracking_application_token_owner_has_position_permission(
    p_token_id UUID,
    p_tenant_id UUID,
    p_resource_type TEXT,
    p_resource_id UUID
) RETURNS BOOLEAN
LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM public.application_tokens token
        WHERE token.id = p_token_id
          AND token.tenant_id = p_tenant_id
          AND token.revoked_at IS NULL
          AND token.expires_at > statement_timestamp()
          AND token.scopes @> ARRAY['positions:read']::TEXT[]
          AND p_resource_type IN ('target', 'device')
          AND CASE WHEN p_resource_type = 'target' THEN EXISTS (
              SELECT 1 FROM public.application_token_targets allowlist
              WHERE allowlist.tenant_id = p_tenant_id
                AND allowlist.application_token_id = token.id
                AND allowlist.target_id = p_resource_id
          ) ELSE EXISTS (
              SELECT 1 FROM public.application_token_devices allowlist
              WHERE allowlist.tenant_id = p_tenant_id
                AND allowlist.application_token_id = token.id
                AND allowlist.device_id = p_resource_id
          ) END
          AND public.gps_tracking_member_has_permission(
              p_tenant_id, token.owner_user_id, p_resource_type, p_resource_id, 'read'
          )
    )
$$;

ALTER FUNCTION gps_tracking_application_token_owner_has_position_permission(UUID, UUID, TEXT, UUID)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_application_token_owner_has_position_permission(UUID, UUID, TEXT, UUID)
    FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_application_token_owner_has_position_permission(UUID, UUID, TEXT, UUID)
    TO gps_tracking_app;
