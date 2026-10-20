# PostgreSQL backup and restore runbook

This is the operational procedure for the GPS Tracker MVP. It does not authorize
production changes. The first production backup/restore must be performed in an
approved isolated environment and recorded before pilot traffic is accepted.

## Backup design

- Use pgBackRest with PostgreSQL/PostGIS. The validated local drill used pgBackRest
  2.59.1 against PostgreSQL 17. Keep the selected pgBackRest release pinned and
  compatible across all repository hosts; upgrade it through a tested change.
- Configure `archive_mode=on`, `wal_level=replica`, and
  `archive_command='pgbackrest --stanza=gps-tracker archive-push %p'` on PostgreSQL.
  `archive_timeout=60s` bounds idle WAL segment switching. Monitor archive failures
  and alert when the latest successful archive is more than five minutes old.
- Use `pgbackrest.conf.example` as the non-secret configuration. `repo1-path` must
  resolve to separately managed backup storage, ideally outside the DB host's
  failure domain. pgBackRest encrypts repository data client-side with AES-256-CBC;
  transport and provider/storage encryption remain additional controls.
- Set `PGBACKREST_REPO1_CIPHER_PASS` through a secret manager or a root-only
  systemd credential/environment file (mode 0600). Keep a separately protected,
  tested recovery copy of the key; a lost key makes encrypted backups unusable.
  Never put the key in this repo, a container image, logs, or the backup repository.
  PostgreSQL connection credentials are separate and should use a restricted
  `.pgpass` file (mode 0600) or local peer access.
- Run one full backup each week and one incremental backup each day. pgBackRest
  archives WAL continuously. The 30-day time-based full retention keeps a baseline
  older than the recovery window; verify `pgbackrest info` still shows the required
  baseline and dependent WAL. Do not manually delete repository files.
- Initialize once, then verify repository and archive operation:

  ```sh
  pgbackrest --stanza=gps-tracker stanza-create
  pgbackrest --stanza=gps-tracker check
  pgbackrest --stanza=gps-tracker --type=full backup
  pgbackrest --stanza=gps-tracker --type=incr backup
  pgbackrest --stanza=gps-tracker info
  ```

  Schedule the full/incremental commands weekly/daily and run `check` daily.
  Alert on any non-zero result, missing expected backup, WAL lag over five minutes,
  repository capacity, and an oldest recoverable point that no longer covers the
  30-day window. Keep backup and key recovery operators separate from normal app
  credentials.

## Isolated point-in-time restore

1. Declare the recovery point and incident time in UTC. Quarantine API, portal,
   ingestion, and scheduled workers. Restore into a fresh isolated PostgreSQL
   instance with no application ingress. Do not overwrite the source DB as a
   recovery test.
2. Install the pinned pgBackRest version and provide the original cipher key via
   the secret mechanism. Mount the repository read-only where supported. Verify
   `pgbackrest info`, repository status, and the target backup/WAL range.
3. Restore to the selected point in time. Substitute an approved UTC timestamp:

   ```sh
   pgbackrest --stanza=gps-tracker --type=time \
     --target='2026-01-01 12:00:00+00' --target-action=promote restore
   ```

   Start PostgreSQL only on the isolated network. Confirm it accepts connections,
   PostGIS and expected schema migrations are present, and the recovered commit
   time is no later than the chosen recovery target. Record actual recovery point,
   incident time, restore start, and ready-for-validation time.
4. Replay each retained deletion tombstone before opening any API. Require the
   case's two-person review, validate tenant/device and UTC bounds, apply an
   idempotent parameterized deletion for the exact approved scope, and record only
   case ID and per-table counts. Tombstones must contain no coordinates, tokens,
   or credential material and remain outside DB snapshots for 45 days. For a
   positions-only tombstone, the transaction pattern is:

   ```sh
   psql "$RESTORE_DATABASE_URL" -v ON_ERROR_STOP=1 \
     -v tenant_id='approved-tenant-uuid' \
     -v device_id='approved-device-uuid' \
     -v from_utc='2026-01-01T00:00:00Z' \
     -v until_utc='2026-01-02T00:00:00Z' <<'SQL'
   BEGIN;
   WITH deleted AS (
     DELETE FROM positions
     WHERE tenant_id=:'tenant_id'::uuid
       AND device_id=:'device_id'::uuid
       AND recorded_at >= :'from_utc'::timestamptz
       AND recorded_at < :'until_utc'::timestamptz
     RETURNING 1
   ) SELECT count(*) AS deleted_positions FROM deleted;
   COMMIT;
   SQL
   ```

   Only run this as the reviewed recovery/operator capability; do not grant it to
   the HTTP app or retention worker. Other object types need their separately
   reviewed deletion scope and must not be inferred from a position tombstone.
5. Run the retention job on the isolated DB using the dedicated
   `RETENTION_DATABASE_URL` principal and `gps_tracking_retention_worker` capability:

   ```sh
   POSITION_RETENTION_DAYS=365 RETENTION_DATABASE_URL='postgres://retention-user@restore-host/tracker' \
     cargo run -p server --bin retentionctl
   ```

   Do not run it with the HTTP role. Require success and confirm no expired
   positions remain beyond the 24-hour lag alarm.
6. Invalidate pre-restore authentication state in one reviewed transaction before
   exposing the service: revoke all auth sessions, unconsumed account/reset tokens,
   pending invitation links, application tokens, and active device credentials;
   consume outstanding auth challenges and recovery codes; increment user
   `auth_version`. Reissue credentials through normal audited flows. This is a
   restore-only operation and must not be added to routine startup. The SQL shape
   below assumes an isolated restore where all old auth state is untrusted:

   ```sql
   BEGIN;
   UPDATE auth_sessions SET revoked_at=COALESCE(revoked_at, clock_timestamp());
   UPDATE account_tokens SET revoked_at=COALESCE(revoked_at, clock_timestamp())
     WHERE consumed_at IS NULL AND revoked_at IS NULL;
   UPDATE invitations SET status='revoked', revoked_at=clock_timestamp()
     WHERE status='pending' AND revoked_at IS NULL;
   UPDATE auth_challenges SET consumed_at=COALESCE(consumed_at, clock_timestamp());
   UPDATE recovery_codes SET consumed_at=COALESCE(consumed_at, clock_timestamp());
   UPDATE application_tokens SET revoked_at=COALESCE(revoked_at, clock_timestamp())
     WHERE revoked_at IS NULL;
   UPDATE device_credentials SET revoked_at=COALESCE(revoked_at, clock_timestamp())
     WHERE revoked_at IS NULL;
   UPDATE users SET auth_version=auth_version+1;
   COMMIT;
   ```
7. Validate organization/tenant rows, at least one active organization admin per
   organization, schema/migration version, and expected device/session access
   policy. Check position counts and ranges without exporting coordinates. Run
   API readiness and negative cross-tenant authorization smoke tests from the
   isolated environment. Only then may an authorized operator route traffic.
8. Keep the source and restored instance isolated until reconciliation and the
   recovery decision are complete. Preserve the encrypted backup and key audit
   trail; do not copy real location data into test reports.

## RPO, RTO, and rollback evidence

The MVP targets RPO <=5 minutes and RTO <=4 hours. For every drill, calculate RPO
from the incident timestamp to the last transaction recovered, and measure RTO
from restore initiation through tombstone replay, retention purge, credential
invalidation, tenant/admin checks, and isolated service readiness. Record actual
commands, pgBackRest version, backup labels, data volume, timings, failures, and
the resulting recovery point. Repeat before production and at least quarterly.

Application rollback is allowed only when the previous image has been tested
against the schema version left by the release. Use expand/contract migrations:
backup/preflight, additive compatible migration, deploy, readiness/smoke; on
failure, run the previous image only if its compatibility check passes. Do not
ship destructive schema changes in the same release. If compatibility is unknown,
keep traffic quarantined and forward-fix; database restore is a separately
approved recovery, not routine code rollback. Capture the old image digest,
migration set, compatibility test, and smoke result in release evidence.

The current local drill has no previous application image to test. Actual image
compatibility is not an OPS-03 acceptance item; collect it as a mandatory
PILOT-01 gate by testing a previous build/image (it need not have been in
production) against the post-migration schema in staging. If the test is missing
or fails, the pilot remains blocked.
