-- The realtime worker needs tenant IDs to enter one RLS-scoped transaction at
-- a time. Expose active IDs only, never tenant names, members, devices or data.
CREATE FUNCTION gps_tracking_active_tenants()
RETURNS TABLE(tenant_id UUID)
LANGUAGE SQL STABLE SECURITY DEFINER
SET search_path = pg_catalog, public SET row_security = off
AS $$
    SELECT o.id FROM public.organizations o
    WHERE o.status='active'
    ORDER BY o.id
$$;
ALTER FUNCTION gps_tracking_active_tenants() OWNER TO gps_tracking_grant_policy_owner;
REVOKE ALL ON FUNCTION gps_tracking_active_tenants() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION gps_tracking_active_tenants() TO gps_tracking_auth;
