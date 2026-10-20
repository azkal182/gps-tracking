# OPS-01 — Purge retensi, audit dan logs

Tanggal: 2026-10-20

## Implementasi

- Added `retentionctl`, a one-shot hourly-scheduler entry point requiring the separate `RETENTION_DATABASE_URL` principal. The storage worker switches to `gps_tracking_retention_worker` transaction-locally and purges positions, audit events after 365 days, idempotency/retry rows, outbox rows, and expired/revoked/consumed auth metadata after seven days. Each transaction is capped at 5,000 rows with a 100 ms pause between full batches; queries select only primary keys and expiry metadata.
- Added narrow worker grants and expiry-only RLS visibility for email outbox. The worker has no location payload read, target mutation, or HTTP/mail capability.
- `retentionctl` exits non-zero on database/purge failure and if the oldest remaining expired position is over 24 hours old. Scheduler failure/non-zero status is the operational alert integration point.
- Updated database operations guidance with hourly scheduling, 14-day journald retention and disk limits, log redaction rules, and a tenant/device/UTC-range hard-deletion and 45-day external tombstone/restore replay procedure.
- Existing generated position expiry keeps UTC calendar-year behavior (including 29 February to 28 February) and API expiry filtering. Storage migration-count assertion now tracks the actual embedded migration list.

## Evidence

- `cargo fmt --all --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo test --workspace` — passed: 13 tests passed, 14 database-dependent tests ignored by default.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — passed: 11 tests.
- `GPS_TEST_DATABASE_URL=postgres://postgres:<disposable>@127.0.0.1:54339/postgres cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` — passed: 4/4 against a fresh disposable PostgreSQL 17/PostGIS 3.5 container. The retention fixture verifies leap-year expiry, expired-position deletion, 365-day audit purge, retry/idempotency expiry, outbox cleanup, seven-day auth metadata cleanup, and that the worker cannot read location payloads or delete live positions/targets.
- `RETENTION_DATABASE_URL=postgres://postgres:<disposable>@127.0.0.1:54339/postgres cargo run -p server --bin retentionctl` — passed against the disposable DB and reported counts only.
- Re-running the worker with a wrong disposable password — exited non-zero with a sanitized database connection error, suitable for scheduler alerting.
- `python3 scripts/project_tracker.py validate` — passed before final status update; render follows this evidence update.

## Batas verifikasi

No production database, scheduler, journald host configuration, hard deletion, tombstone storage, or restore was changed or exercised. Deployment must provision a login principal as a member of the retention capability, schedule the command hourly, route non-zero status to alerting, configure and verify 14-day runtime log retention, and keep real deletion tombstones outside DB snapshots. These are deployment/operator actions, not evidence of production readiness.

## Langkah berikut

OPS-02: security limits dan observability.
