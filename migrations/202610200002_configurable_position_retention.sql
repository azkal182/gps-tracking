-- Position expiry is materialized per row so deployments can configure retention
-- for newly ingested points without changing global database state.
ALTER TABLE positions
    ALTER COLUMN expires_at DROP EXPRESSION;

CREATE FUNCTION position_expires_at_utc_days(recorded_at TIMESTAMPTZ, retention_days INTEGER)
RETURNS TIMESTAMPTZ
LANGUAGE SQL
IMMUTABLE
STRICT
PARALLEL SAFE
AS $$
    SELECT ((recorded_at AT TIME ZONE 'UTC') + (retention_days * INTERVAL '1 day')) AT TIME ZONE 'UTC'
$$;

CREATE FUNCTION positions_apply_default_expiry()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.expires_at IS NULL THEN
        NEW.expires_at := position_expires_at_utc(NEW.recorded_at);
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER positions_apply_default_expiry_trigger
    BEFORE INSERT ON positions
    FOR EACH ROW EXECUTE FUNCTION positions_apply_default_expiry();

ALTER TABLE positions
    ALTER COLUMN expires_at SET NOT NULL,
    ADD CONSTRAINT positions_expiry_after_recorded_check CHECK (expires_at > recorded_at);

-- The scheduled retention principal can also expire rows written under a longer
-- prior policy. Its transaction-local setting is never available to app roles.
DROP POLICY gps_tracking_retention_positions_select ON positions;
DROP POLICY gps_tracking_retention_positions_delete ON positions;
CREATE POLICY gps_tracking_retention_positions_select ON positions
    FOR SELECT TO gps_tracking_retention_worker
    USING (
        expires_at <= pg_catalog.statement_timestamp()
        OR recorded_at < pg_catalog.statement_timestamp() - make_interval(days => COALESCE(NULLIF(current_setting('gps.position_retention_days', true), '')::integer, 365))
    );
CREATE POLICY gps_tracking_retention_positions_delete ON positions
    FOR DELETE TO gps_tracking_retention_worker
    USING (
        expires_at <= pg_catalog.statement_timestamp()
        OR recorded_at < pg_catalog.statement_timestamp() - make_interval(days => COALESCE(NULLIF(current_setting('gps.position_retention_days', true), '')::integer, 365))
    );
GRANT SELECT (id, expires_at, recorded_at), DELETE ON positions TO gps_tracking_retention_worker;
