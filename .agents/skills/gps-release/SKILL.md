---
name: gps-release
description: "Build foundation/CI/deployment tooling or verify GPS MVP security, load, retention, backup/restore and release readiness; use for the repository\u2019s operational tasks, not automatic production deployment."
---

# GPS Verification and Release

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-implementation-plan.md](../../../docs/gps-tracking-implementation-plan.md)
- [gps-tracking-database-operations.md](../../../docs/gps-tracking-database-operations.md)
- [gps-tracking-baseline-final.md](../../../docs/gps-tracking-baseline-final.md)
- [gps-tracking-technical-decisions.md](../../../docs/gps-tracking-technical-decisions.md)

## Workflow

Read the selected task and operations/release gates. Separate local packaging, staging verification and production rollout.

- Foundation uses Cargo/pnpm with lockfiles, Compose PostgreSQL/PostGIS/Mailpit, Caddy and multi-stage static portal/Rust image. Never send /v1 failures through SPA fallback.
- CI should establish actual reproducible formatting/lint/typecheck/test/migration/contract commands; run appropriate checks, not invented scripts.
- Readiness requires negative auth/tenant tests, concurrent tracking cases, expiry/purge and recovery evidence. A passing doc validator or mock does not satisfy these gates.
- Load results include hardware, seed size, sample rate, burst duration, command, measurement and errors; don't equate 100 device fixtures with proven one-year capacity.
- Backup/restore uses isolated environment and verified tombstones, expiry purge, auth invalidation and keys. Measure RPO/RTO; do not mark passed from configuration alone.
- Inspect all required backlog tasks/evidence before claiming MVP ready. Local-ready is separate from pilot-live. Only perform deployment, provider purchase, real email or live mutation when the user/session authorizes it; otherwise report the precise external prerequisite without falsely completing P11.
- Record failures/limitations and next task in execution evidence; never silently reduce release criteria to obtain green status.
