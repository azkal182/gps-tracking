---
name: gps-storage
description: "Implement SQLx migrations, tenant RLS, database constraints, transactions, indexing and retention for this GPS tracking project."
---

# GPS Storage

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-database-operations.md](../../../docs/gps-tracking-database-operations.md)
- [gps-tracking-app-spec.md](../../../docs/gps-tracking-app-spec.md)
- [gps-tracking-api-contract.md](../../../docs/gps-tracking-api-contract.md)

## Workflow

Translate the schema design into executable migrations, not comments claiming constraints exist. Read operations §1–4 and the relevant domain invariant before editing.

- Use composite tenant FKs and RLS with non-owner role/FORCE RLS and transaction-local tenant context. Test pool reuse and missing context; object authorization remains service-layer work.
- Apply one documented lock order across session start/end, ingestion, device disable/credential rotation and org suspension. Clock after lock matters; enforce one active session and temporal non-overlap in DB.
- Dedupe concurrent writers using uniqueness and canonical fingerprint; batch DB failures roll back, semantic item rejection stays per-item. Replayed stored messages are checked before time/policy reevaluation.
- Keep UTC calendar-year expires_at, expiry filtering on reads and bounded purge. Latest target query must intersect current session; old backfill cannot regress latest.
- Start unpartitioned per decision; do not add partitioning casually because global message uniqueness needs a proven migration/ledger design.
- Run fresh/upgrade migrations and negative/concurrency tests on disposable PostgreSQL/PostGIS. Never reset a user's database as a test convenience. Record query plans/seed volume for performance claims; a unit test does not prove RLS or DB constraints.
