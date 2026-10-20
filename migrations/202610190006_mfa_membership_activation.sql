-- A verified first TOTP enrollment may activate pending memberships without
-- giving the auth runtime general write access to tenant membership data.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_tracking_mfa_activation_owner') THEN
        CREATE ROLE gps_tracking_mfa_activation_owner NOLOGIN BYPASSRLS;
    END IF;
END
$$;
ALTER ROLE gps_tracking_mfa_activation_owner NOLOGIN BYPASSRLS;
GRANT USAGE ON SCHEMA public TO gps_tracking_mfa_activation_owner;
GRANT SELECT (user_id, enabled_at) ON mfa_factors TO gps_tracking_mfa_activation_owner;
GRANT SELECT (user_id, status) ON memberships TO gps_tracking_mfa_activation_owner;
GRANT UPDATE (status, updated_at) ON memberships TO gps_tracking_mfa_activation_owner;

CREATE FUNCTION gps_tracking_activate_pending_mfa_memberships(p_user_id UUID)
RETURNS INTEGER
LANGUAGE PLPGSQL
SECURITY DEFINER
SET search_path = pg_catalog, public
SET row_security = off
AS $$
DECLARE
    activated_count INTEGER;
BEGIN
    UPDATE public.memberships AS membership
       SET status = 'active', updated_at = clock_timestamp()
     WHERE membership.user_id = p_user_id
       AND membership.status = 'pending_mfa'
       AND EXISTS (
           SELECT 1 FROM public.mfa_factors AS factor
            WHERE factor.user_id = p_user_id AND factor.enabled_at IS NOT NULL
       );
    GET DIAGNOSTICS activated_count = ROW_COUNT;
    RETURN activated_count;
END
$$;
ALTER FUNCTION gps_tracking_activate_pending_mfa_memberships(UUID)
    OWNER TO gps_tracking_mfa_activation_owner;
REVOKE ALL ON FUNCTION gps_tracking_activate_pending_mfa_memberships(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_activate_pending_mfa_memberships(UUID)
    TO gps_tracking_auth;
