-- Resolve an application bearer digest without exposing the digest column to the
-- ordinary API role. Resource scope/grant checks remain per request through
-- gps_tracking_application_token_owner_has_permission.
GRANT SELECT (token_hash) ON application_tokens TO gps_tracking_grant_policy_owner;

CREATE FUNCTION gps_tracking_lookup_application_token(p_token_hash BYTEA)
RETURNS TABLE (token_id UUID, tenant_id UUID, owner_user_id UUID, scopes TEXT[])
LANGUAGE SQL
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
    SELECT t.id, t.tenant_id, t.owner_user_id, t.scopes
    FROM public.application_tokens t
    JOIN public.memberships m
      ON m.tenant_id=t.tenant_id AND m.user_id=t.owner_user_id
    JOIN public.users u ON u.id=t.owner_user_id
    JOIN public.organizations o ON o.id=t.tenant_id
    WHERE t.token_hash=p_token_hash
      AND pg_catalog.octet_length(p_token_hash)=32
      AND t.revoked_at IS NULL
      AND t.expires_at>pg_catalog.statement_timestamp()
      AND m.status='active'
      AND u.status='active'
      AND o.status='active'
    LIMIT 1
$$;
ALTER FUNCTION gps_tracking_lookup_application_token(BYTEA)
    OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_lookup_application_token(BYTEA) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_lookup_application_token(BYTEA)
    TO gps_tracking_app;
