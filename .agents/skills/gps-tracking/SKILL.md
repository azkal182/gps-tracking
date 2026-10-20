---
name: gps-tracking
description: "Implement or fix GPS target/device sessions, HTTP ingestion, adaptive-position heartbeat, history/latest queries and authorized WebSocket behavior in this repository."
---

# GPS Tracking Core

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-app-spec.md](../../../docs/gps-tracking-app-spec.md)
- [gps-tracking-api-contract.md](../../../docs/gps-tracking-api-contract.md)
- [gps-tracking-database-operations.md](../../../docs/gps-tracking-database-operations.md)

## Workflow

Select the FR/BR/AC IDs for the change and trace them through HTTP adapter, domain service, transaction and event output.

- Keep Target distinct from Device and session intervals [start,end). Session-only accepts only valid temporal association including permitted late points; future points use TIMESTAMP_NOT_READY retry semantics.
- Heartbeat has no coordinates and does not create positions, extend sessions or update freshness. Use server contact time; distinguish session active/inactive, connectivity and position age.
- Commit before stored or broadcasts; dedupe emits no location update. Evaluate latest device and latest active session separately, guarding out-of-order packets and same timestamp ID ordering.
- REST snapshot is authoritative; best-effort WS requires resubscribe/snapshot, tenant/session binding and observed_at/position order. Check authorization at send time and invalidate revoked connections.
- Test active/inactive boundaries, future/late/retry conflicts, concurrent session changes, heartbeat without session, stale-but-online, unknown/timeout/restart, expiry and cross-tenant WS.
- Protocols beyond HTTP are post-MVP; do not build hardware adapters or client movement algorithms here.
