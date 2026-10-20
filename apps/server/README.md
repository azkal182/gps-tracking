# Server foundation

Run locally with `cargo run -p server`. The server reads `BIND_ADDR` (default
`0.0.0.0:8080`), optional `DATABASE_URL`, `TRACKING_DATABASE_URL`, and `PUBLIC_ORIGIN` (default
`http://localhost:5173`). Set `DATABASE_URL` to the runtime login principal
granted `gps_tracking_auth` to enable auth and portal-management routes; without it, those endpoints
return service unavailable. Set `TRACKING_DATABASE_URL` to a separate runtime principal
granted `gps_tracking_app` to enable target/device APIs. The tracking role never handles
browser authentication, and the auth role has no direct tracking-table access. When `DATABASE_URL` is set, configure
`MFA_ENCRYPTION_KEY` as 32 random bytes encoded URL-safe Base64 without padding
and `MFA_ENCRYPTION_KEY_VERSION` as a positive integer (default `1`). MFA
enrollment is unavailable unless a valid key is configured. During key rotation,
retain each old key as `MFA_ENCRYPTION_KEY_<version>` until all factors using it
have been re-encrypted or removed. Supply keys through a secret manager in
deployment; never commit them. Do not run the service as a superuser. Secrets use
redacted `Debug` output and must not be logged or included in error messages.
Password-reset requests require `OUTBOX_ENCRYPTION_KEY`, a separate 32-byte
URL-safe Base64 key without padding, to encrypt the reset link before it enters
the email outbox. Retain previous outbox keys for at least the 72-hour outbox
retention window during rotation. Configure `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS`,
and `SMTP_FROM`; `SMTP_TLS` is `tls` for implicit TLS or `starttls` for STARTTLS.
Plain SMTP is allowed only for local Mailpit/loopback and cannot use credentials.
The optional `MAILER_DATABASE_URL` must be a distinct login principal granted
`gps_tracking_mailer`; this capability can read, send and delete outbox rows and
has no access to location tables. If unset, email stays queued. Use
`AUTH_DATABASE_URL`, a distinct auth-capability login, for
`cargo run -p server --bin authctl -- ...` operator commands.
Password-reset email requests are limited in process to 3 per normalized
recipient and 20 per client IP in a rolling hour. Direct peer IP is used by
default. Set `TRUSTED_PROXY_IP` only to the exact reverse-proxy peer address to
honor its `X-Real-IP`; forwarded headers from other peers are ignored. The
limiter is intentionally single-instance, matching the current deployment plan.

`authctl bootstrap-admin` is one-time, creates a `pending_activation`
platform-admin with no usable password, and queues a 72-hour activation email.
The recipient sets the initial password through `/v1/auth/activate`; MFA is
required on subsequent management access. `authctl operator-recovery` requires
email, operator, reason, unique case ID and a new `--token-file` path. It writes
a cryptographically random 15-minute authorization token to a new Unix file
with mode `0600`, stores only its digest, queues a 15-minute email proof, and
writes the case/audit record. The operator shares that authorization with the
verified account owner through a separately verified channel, then removes the
token file. The
recovery endpoint resets the password, revokes sessions and factors, and grants
only the five-minute MFA-enrollment challenge. It never issues a full session.

## Database role provisioning

Migration `202610190004_tenant_rls.sql` creates capability roles without login.
Provision separate login principals through the deployment secret/bootstrap
process, then grant `gps_tracking_app` only to the HTTP service principal and
`gps_tracking_retention_worker` only to the retention worker principal. Do not
run the HTTP service as a superuser, table owner, schema owner, or
`gps_tracking_device_lookup_owner`. The latter is a `NOLOGIN BYPASSRLS` owner
used only by the fixed device-credential lookup function; never grant membership
in it. Authentication uses a separate principal granted only
`gps_tracking_auth`, which can reach global identity/session material and
tenant-scoped memberships, plus a boolean-only MFA-policy lookup, but has no
tracking-data privileges. The MFA-policy function is owned by the separate
`gps_tracking_auth_policy_owner` capability, which can read only the account
policy columns it needs. Platform
provisioning uses a separate principal granted only
`gps_tracking_platform_provisioner`; that capability can create organizations,
invitations, outbox messages, and audit events, but has no location-table
privileges. Configure `PLATFORM_DATABASE_URL` for management routes that
create/list/suspend organizations, using a distinct login principal with this
capability. Organization invitation and metadata endpoints use the auth
principal plus transaction-local tenant context and FORCE RLS. Keep
migration/bootstrap credentials separate from runtime credentials.

Every tenant query/mutation must use `tracking_storage::begin_tenant_transaction`
or set context on an existing transaction with
`tracking_storage::set_tenant_context`. The helper sets `app.tenant_id` locally
to that transaction; session-level tenant settings are not allowed. This is a
tenant boundary only; service code must still enforce membership, object grants,
scopes, and resource ownership.

`/healthz` reports that the HTTP process responds. `/readyz` currently reflects
only the process readiness flag. A later storage task must connect readiness to
the database before the service can claim dependency readiness. Restrict
`/readyz` to the operations network at the deployment boundary.

The server drains through Axum graceful shutdown on SIGINT/SIGTERM. The
deployment runtime should impose its own termination deadline.

Device ingestion credentials are created only by the authenticated portal
rotation endpoint. The plaintext token is returned once with
`Cache-Control: no-store`; only its SHA-256 digest is stored. Store the token in
the device's secret configuration and do not copy it into logs or support
reports. Setting a device's `enabled` field to `false` revokes all active
credentials in the same transaction. Re-enabling only changes device state; it
never restores a revoked token, so issue a new credential after re-enabling.
