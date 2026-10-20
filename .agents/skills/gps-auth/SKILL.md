---
name: gps-auth
description: "Implement or verify GPS portal login, TOTP/recovery, invitations, membership/grants, credential lifecycle and REST/WebSocket tenant authorization in this repository."
---

# GPS Auth and Tenants

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-management-api.md](../../../docs/gps-tracking-management-api.md)
- [gps-tracking-technical-decisions.md](../../../docs/gps-tracking-technical-decisions.md)
- [gps-tracking-database-operations.md](../../../docs/gps-tracking-database-operations.md)
- [gps-tracking-portal-spec.md](../../../docs/gps-tracking-portal-spec.md)

## Workflow

Read the management endpoint contract plus technical decisions for exact TTL/roles. Follow storage rules for atomic consumption and locking.

- Never merge auth session, tracking session, application token and device token. Limited password/MFA enrollment challenge must not access application data.
- Enforce membership/grants on every request and event delivery. Platform admin does not implicitly access tenant coordinates. Org context is explicit per request/tab.
- Consume TOTP steps, recovery codes, invites/reset tokens atomically; password reset preserves MFA. Pending_mfa onboarding must reach enrollment without granting privileged management access.
- Rotate/revoke secret lifecycles exactly as planned; prevent last-admin race and token resurrection after device re-enable. Reset/recovery closes affected WS and sessions.
- Use library crypto, Argon2id, server sessions/CSRF and encrypted TOTP secrets. Do not log bodies/cookies or store credentials in frontend persistence.
- Add negative tests for bypass/replay, wrong tenant/role/principal, disabled device, current grants after token issuance, concurrent last-admin/rotation, and revocation on open WS. Document recovery/operator flows with synthetic fixtures only.
- Real invite emails, production secret rotation and account mutation require existing task authorization; use Mailpit/local fixtures while developing.
