# Backlog sampai MVP

> Generated dari backlog.json; update status hanya pada JSON.

P00 = setup; P01–P11 wajib MVP; POST di luar MVP. Todo belum tentu dependency-ready.

## P00 — Planning dan workflow agent

### PLAN-01 — Finalisasi baseline planning

Status: **done** · Scope: setup · Owner: codex

Dependensi: tidak ada

Skill: `$gps-plan`

Acceptance:
- Baseline v1.0, keputusan dan batas validasi tersedia; bukan klaim kode selesai.
- Planning terhubung di indeks, JSON contoh dan link lokal diperiksa.

Sumber: [gps-tracking-baseline-final.md](../../docs/gps-tracking-baseline-final.md), [README.md](../../docs/README.md)

Evidence: [PLAN-01.md](../../docs/execution/evidence/PLAN-01.md)

### AGENT-01 — AGENTS, skill lokal dan tracker progres

Status: **done** · Scope: setup · Owner: codex

Dependensi: PLAN-01

Skill: `$gps-plan`, `$gps-backlog`

Acceptance:
- AGENTS.md, delapan skill repo dan coverage semua planning tersedia; tidak ada instalasi global.
- Tracker validate/render/status/next, validator skills dan negative tests tracker lulus.
- Backlog memisahkan planning selesai dari implementasi yang belum dimulai.

Sumber: [gps-tracking-implementation-plan.md](../../docs/gps-tracking-implementation-plan.md)

Evidence: [AGENT-01.md](../../docs/execution/evidence/AGENT-01.md)

## P01 — Foundation proyek

### FOUND-01 — Scaffold workspace Rust dan health

Status: **done** · Scope: mvp · Owner: codex

Dependensi: AGENT-01

Skill: `$gps-release`

Acceptance:
- Cargo workspace apps/server dan crates sesuai baseline; config typed dan secret redaction.
- healthz/readyz, graceful shutdown, fmt/clippy/build/test tersedia dan lulus.

Sumber: [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: [FOUND-01.md](../../docs/execution/evidence/FOUND-01.md)

### FOUND-02 — Scaffold portal dan tooling frontend

Status: **done** · Scope: mvp · Owner: codex

Dependensi: FOUND-01

Skill: `$gps-portal`, `$gps-release`

Acceptance:
- React/Vite TS strict, Tailwind/shadcn, pnpm lockfile dan scripts lint/typecheck/test/build tersedia.
- Shell dengan route/loading/error, dev API/WS proxy; belum mengklaim fitur auth selesai.

Sumber: [gps-tracking-frontend-stack.md](../../docs/gps-tracking-frontend-stack.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [FOUND-02.md](../../docs/execution/evidence/FOUND-02.md)

### FOUND-03 — Environment lokal dan CI dasar

Status: **done** · Scope: mvp · Owner: codex

Dependensi: FOUND-01, FOUND-02

Skill: `$gps-release`

Acceptance:
- Compose disposable PG/PostGIS/Mailpit dan sample env tanpa secret; dokumentasi perintah.
- CI build/lint/typecheck dan unit tests backend/frontend, dependency versions pinned.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [FOUND-03.md](../../docs/execution/evidence/FOUND-03.md)

### DEVSEED-01 — Seed sintetis lokal untuk uji portal browser

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: FOUND-03

Skill: `$gps-auth`, `$gps-backlog`

Acceptance:
- Seed hanya menerima localhost *_dev/*_test, memakai password tersembunyi/env dan Argon2id, menerapkan migration serta idempotent.
- Akun admin lokal MFA-normal dan dua organisasi dengan target/device/session/position sintetis dapat dipakai di portal.
- Disposable PostGIS memverifikasi migration, row counts dan rerun no-op; panduan browser tersedia.

Sumber: [development-environment.md](../../docs/execution/development-environment.md), [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md)

Evidence: [DEVSEED-01.md](../../docs/execution/evidence/DEVSEED-01.md)

## P02 — Kontrak API dan WS

### CONTRACT-01 — OpenAPI tracking lengkap

Status: **done** · Scope: mvp · Owner: codex

Dependensi: FOUND-03

Skill: `$gps-contracts`

Acceptance:
- Semua route tracking/heartbeat, fields tenant/status dan errors tercakup OpenAPI 3.1.
- Contoh valid terhadap schema; idempotency/header auth/limit/pagination eksplisit.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md)

Evidence: [CONTRACT-01.md](../../docs/execution/evidence/CONTRACT-01.md)

### CONTRACT-02 — OpenAPI management dan schema WS

Status: **done** · Scope: mvp · Owner: codex

Dependensi: CONTRACT-01

Skill: `$gps-contracts`, `$gps-auth`

Acceptance:
- Auth/MFA/challenges, invite/platform/grants/token/audit routes tercakup.
- Schema event/subscription/ticket WS eksplisit termasuk observed_at dan access rules.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md)

Evidence: [CONTRACT-02.md](../../docs/execution/evidence/CONTRACT-02.md)

### CONTRACT-03 — Generate client dan CI contract checks

Status: **done** · Scope: mvp · Owner: codex

Dependensi: CONTRACT-02

Skill: `$gps-contracts`, `$gps-portal`

Acceptance:
- TS types/openapi-fetch client dihasilkan dari kontrak dan dapat dipakai portal.
- CI validation/example/generated diff check mencegah drift; tidak hand-edit generated output.

Sumber: [gps-tracking-frontend-stack.md](../../docs/gps-tracking-frontend-stack.md), [gps-tracking-implementation-plan.md](../../docs/gps-tracking-implementation-plan.md)

Evidence: [CONTRACT-03.md](../../docs/execution/evidence/CONTRACT-03.md)

## P03 — Database dan isolasi tenant

### DB-01 — Migrasi identity, tenant dan auth

Status: **done** · Scope: mvp · Owner: codex

Dependensi: CONTRACT-02

Skill: `$gps-storage`, `$gps-auth`

Acceptance:
- Tables identity/membership/auth/factors/token/outbox dan constraint uniqueness diterapkan.
- Migration fresh/upgrade, atomic token/code consumption dan missing tenant tests lulus.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md), [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md)

Evidence: [DB-01.md](../../docs/execution/evidence/DB-01.md)

### DB-02 — Migrasi domain, positions dan locking

Status: **done** · Scope: mvp · Owner: codex

Dependensi: DB-01

Skill: `$gps-storage`

Acceptance:
- Targets/devices/session/positions/credentials/idempotency/audit termasuk tenant composite FKs dan indexes.
- Active-session uniqueness/exclusion, canonical-year expiry dan lock order diuji concurrency.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: [DB-02.md](../../docs/execution/evidence/DB-02.md)

### DB-03 — RLS dan repositories tenant-scoped

Status: **done** · Scope: mvp · Owner: codex

Dependensi: DB-02

Skill: `$gps-storage`, `$gps-auth`

Acceptance:
- Non-owner FORCE RLS, SET LOCAL context, token lookup sempit dan worker role diterapkan.
- Cross-tenant FK/read/write, missing context, pool reuse, owner bypass tests lulus pada PostgreSQL.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [DB-03.md](../../docs/execution/evidence/DB-03.md)

## P04 — Authentication dan recovery

### AUTH-01 — Password login, cookie sesi dan CSRF

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DB-03, CONTRACT-03

Skill: `$gps-auth`

Acceptance:
- Argon2id, strict limited challenges, cookie flags, idle/absolute expiry dan CSRF/Origin diterapkan.
- Wrong password/principal, login CSRF, timeout/logout/session revoke dan no secret logging diuji.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [AUTH-01.md](../../docs/execution/evidence/AUTH-01.md)

### AUTH-02 — TOTP enrollment dan recovery

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: AUTH-01

Skill: `$gps-auth`

Acceptance:
- TOTP encrypted secret, replay-step guard, mandatory MFA dan pending_mfa flow diterapkan.
- Concurrent code replay/recovery, password-only bypass, recent auth dan recovery re-enrollment tests lulus.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [AUTH-02.md](../../docs/execution/evidence/AUTH-02.md)

### AUTH-03 — Email reset/aktivasi dan bootstrap CLI

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: AUTH-02

Skill: `$gps-auth`, `$gps-release`

Acceptance:
- SMTP outbox/Mailpit, expiring one-use links, reset preserving MFA dan CLI bootstrap/operator recovery.
- Link scanner/replay/expiry, matching existing account, secret redaction dan failed-email recovery diuji.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [AUTH-03.md](../../docs/execution/evidence/AUTH-03.md)

## P05 — Organisasi, grants dan onboarding

### ORG-01 — Organisasi dan invitation lifecycle

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: AUTH-03

Skill: `$gps-auth`

Acceptance:
- Platform create/suspend/reactivate, invite accept/resend/revoke dan metadata endpoints selesai.
- Platform tidak mendapat koordinat; org suspend memblokir request, pending admin enrollment dan invite mismatch diuji.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md)

Evidence: [ORG-01.md](../../docs/execution/evidence/ORG-01.md)

### ORG-02 — Membership, grant dan peran

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: ORG-01

Skill: `$gps-auth`, `$gps-storage`

Acceptance:
- Role/grant checks, creator grants, last-admin lock dan request tenant per tab implemented.
- Cross-tenant grant, viewer write, concurrent last-admin, perubahan grant dan integration-owner checks diuji.
- Boolean MFA policy lookup recognizes active operator credentials:write grants without exposing tracking data; grant activation, revocation/downgrade, cross-tenant isolation and login policy are tested.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [ORG-02.md](../../docs/execution/evidence/ORG-02.md)

### ORG-03 — Portal login/onboarding dan organisasi

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: ORG-02, FOUND-02

Skill: `$gps-portal`, `$gps-auth`

Acceptance:
- Login/MFA/recovery, sessions list/revoke, undangan, anggota dan organisasi aktif terintegrasi API.
- Auth-portal acceptance dan tenant switch multi-tab/late response tests lulus tanpa token persistence.

Sumber: [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-frontend-stack.md](../../docs/gps-tracking-frontend-stack.md)

Evidence: [ORG-03.md](../../docs/execution/evidence/ORG-03.md)

## P06 — Target/device dan kredensial

### DEVICE-01 — CRUD terbatas target/device

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: ORG-02

Skill: `$gps-tracking`, `$gps-auth`

Acceptance:
- Create/list/detail/PATCH sesuai schema, unique external_ref/identifier, grants creator dan filters.
- No cross-tenant/read without grant, validation metadata/name, conflict/errors teruji.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md)

Evidence: [DEVICE-01.md](../../docs/execution/evidence/DEVICE-01.md)

### DEVICE-02 — Provisioning, rotasi dan disable device

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DEVICE-01

Skill: `$gps-auth`, `$gps-storage`

Acceptance:
- Secret sekali, one-active-token, recent MFA credential permission dan lifecycle enable/re-enable.
- Rotation/disable concurrent ingestion serialization, old token reject, viewer/operator permission tests lulus.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [DEVICE-02.md](../../docs/execution/evidence/DEVICE-02.md)

### REFAC-01 — Struktur modul server dan pemisahan test

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DEVICE-02

Skill: `$gps-plan`, `$gps-backlog`

Acceptance:
- Konvensi struktur folder source, domain dan test didokumentasikan sebagai panduan refactor bertahap.
- Handler target/device dan test server dipisah dari lib.rs tanpa mengubah route atau perilaku API.
- Workspace Rust diformat, lulus clippy dan seluruh test yang dapat dijalankan; bukti mencatat test yang memerlukan PostgreSQL bila tidak tersedia.

Sumber: [STRUCTURE.md](../../docs/execution/STRUCTURE.md), [gps-tracking-implementation-plan.md](../../docs/gps-tracking-implementation-plan.md)

Evidence: [REFAC-01.md](../../docs/execution/evidence/REFAC-01.md)

### DEVICE-03 — Application tokens dan audit query

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DEVICE-02

Skill: `$gps-auth`, `$gps-contracts`

Acceptance:
- Tenant/owner-scoped limited integration tokens, expiry/revoke dan admin metadata/audit endpoints.
- Tidak bisa menerbitkan credential/manage accounts, owner permission recheck, audit redaction diuji.

Sumber: [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [DEVICE-03.md](../../docs/execution/evidence/DEVICE-03.md)

## P07 — Session dan ingestion

### TRACK-01 — Tracking session lifecycle

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DEVICE-03

Skill: `$gps-tracking`, `$gps-storage`

Acceptance:
- Create/list/detail/end, wajib Idempotency-Key dan auth replay, server clocks setelah lock.
- AC-01/02/10/12, overlaps, concurrent end dan boundaries termasuk zero-duration lulus.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: [TRACK-01.md](../../docs/execution/evidence/TRACK-01.md)

### TRACK-02 — HTTP ingestion dan canonical validation

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: TRACK-01

Skill: `$gps-tracking`, `$gps-storage`

Acceptance:
- Batch/schema/measurements/message hash, session_only/always, late/future retry serta transaction atomicity.
- AC-03/04/05/06/08, DB failure no stored, duplicate/conflict races dan wrong-device token lulus.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: [TRACK-02.md](../../docs/execution/evidence/TRACK-02.md)

### TRACK-03 — Latest/history repositories dan expiry

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: TRACK-02

Skill: `$gps-tracking`, `$gps-storage`

Acceptance:
- Temporal history, deterministic sort/cursor, latest target/device terpisah dan expiry filter.
- AC-07, out-of-order/tie IDs, previous-session visibility, cursor tenant/filter mismatch dan calendar expiry lulus.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [TRACK-03.md](../../docs/execution/evidence/TRACK-03.md)

## P08 — Heartbeat, query dan realtime

### LIVE-01 — Heartbeat dan status terpisah

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: TRACK-03

Skill: `$gps-tracking`

Acceptance:
- Heartbeat tanpa koordinat/sesi, monotonic contact, 60/180 dan freshness terpisah.
- AC-13/14, idle/stale-online/no_position/unknown, rejected batch contact dan disabled device tests.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: [LIVE-01.md](../../docs/execution/evidence/LIVE-01.md)

### LIVE-02 — WS ticket, subscriptions dan timeout

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: LIVE-01, ORG-03

Skill: `$gps-tracking`, `$gps-auth`

Acceptance:
- Single-use ticket session/tenant binding, Origin, subscriptions replace atomik, queue bound dan worker timeout.
- Cross-tenant/unauthorized ID, revocation open WS, expired auth, reconnect/restart serta slow consumer diuji.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [LIVE-02.md](../../docs/execution/evidence/LIVE-02.md)

### LIVE-03 — Rekonsiliasi event dan snapshot

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: LIVE-02

Skill: `$gps-tracking`, `$gps-contracts`

Acceptance:
- Post-commit events, target projection dan observed_at/position ordering sesuai schema.
- AC-09/11, duplicate no event, snapshot race, target reassignment dan backfill event tests lulus.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: [LIVE-03.md](../../docs/execution/evidence/LIVE-03.md)

## P09 — Portal management dan peta

### PORTAL-01 — Halaman target/device/token/session

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: LIVE-03, ORG-03

Skill: `$gps-portal`

Acceptance:
- Management UI menggunakan effective grants pengguna untuk kontrol create/write/credential/session per organisasi dan objek; token sekali tampil, active-session requirements dan lifecycle controls tetap diikuti.
- Bagian provisioning/session AC-PORT-01/02/03/04/09, error/conflict/empty states dan API-direct negative permission tested. Verifikasi map dan histori end-to-end AC-PORT-01/09 didelegasikan ke PORTAL-02/03.

Sumber: [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md)

Evidence: [PORTAL-01.md](../../docs/execution/evidence/PORTAL-01.md)

### PORTAL-02 — Peta live dan status kontak

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: PORTAL-01

Skill: `$gps-portal`

Acceptance:
- MapLibre/MapTiler integration konfigurabel, attribution, current-session map dan separate status.
- AC-PORT-01 live-position journey dan AC-PORT-05/07, provider failure fallback, tenant switch and stale marker behavior E2E diuji.

Sumber: [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: [PORTAL-02.md](../../docs/execution/evidence/PORTAL-02.md)

### PORTAL-03 — Histori, filter dan audit portal

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: PORTAL-02

Skill: `$gps-portal`

Acceptance:
- History device/session, timezone, cursor tables, cap map 10000 partial label dan audit admin.
- AC-PORT-01 history journey dan AC-PORT-06/08/09, date boundaries/pagination/partial data, retained authorized history after device disable, no persisted coordinates diuji.

Sumber: [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-frontend-stack.md](../../docs/gps-tracking-frontend-stack.md)

Evidence: [PORTAL-03.md](../../docs/execution/evidence/PORTAL-03.md)

### PORTAL-04 — Portal manajemen organisasi, anggota dan grants

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: ORG-01, ORG-02, ORG-03, PORTAL-01

Skill: `$gps-portal`, `$gps-auth`, `$gps-contracts`

Acceptance:
- Platform admin memiliki halaman untuk daftar, membuat organisasi beserta undangan admin awal, dan suspend/reactivate organisasi; UI hanya metadata organisasi, backend tetap sumber otorisasi dan recent MFA diwajibkan sesuai kontrak.
- Admin organisasi dapat mengubah role/status dan mencabut membership serta membaca/mengganti grants anggota; UI menegakkan recent authentication, last-admin protection tetap server-side, dan grant editor hanya memakai resource tenant aktif.
- Status hak platform yang diperlukan untuk navigasi tersedia dari profil pengguna sendiri dengan kontrak eksplisit; nilai UI tidak menjadi pengganti authorization backend.
- Komponen memiliki empty/loading/error/success states, konfirmasi tindakan berisiko, query cache terikat tenant, dan test untuk platform/non-platform, tenant switch, member/grant mutations dan error.

Sumber: [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md)

Evidence: [PORTAL-04.md](../../docs/execution/evidence/PORTAL-04.md)

### PORTAL-05 — Portal detail dan lifecycle target/device yang lengkap

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DEVICE-01, DEVICE-02, DEVICE-03, PORTAL-01

Skill: `$gps-portal`, `$gps-auth`

Acceptance:
- Target dapat dibuka sebagai detail dan create/update mengelola name, external_ref dan metadata; tidak menambahkan hard delete yang bertentangan dengan lifecycle histori.
- Device dapat dilihat detail dan diedit pada name, metadata, ingestion_policy dan enabled; protocol/identifier tetap immutable. Daftar credential menunjukkan metadata saja dan mendukung revoke; secret token hanya tampil sekali setelah rotasi.
- Daftar target/device memakai query/filter yang didukung API serta cursor pagination tanpa melewati tenant atau grant; tracking sessions memakai filter/cursor yang didukung API.
- Disable device yang mencabut credential, credential revoke, dan end session memiliki konfirmasi yang jelas; form memberi feedback dan error/conflict actionable.
- Test mencakup edit field, cursor/filter changes, no-secret credential listing, object grants, tenant switch, destructive confirmation dan API error.

Sumber: [gps-tracking-portal-spec.md](../../docs/gps-tracking-portal-spec.md), [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md)

Evidence: [PORTAL-05.md](../../docs/execution/evidence/PORTAL-05.md)

## P10 — Hardening dan readiness

### OPS-01 — Purge retensi, audit dan logs

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: PORTAL-03

Skill: `$gps-storage`, `$gps-release`

Acceptance:
- Purge bounded/expiry reads, 365d audit/14d logs, outbox cleanup dan tombstone procedure.
- Calendar/leap-year expiry, retry retention, purge failure alarm dan data deletion fixtures teruji.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [OPS-01.md](../../docs/execution/evidence/OPS-01.md)

### OPS-02 — Security limits dan observability

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: OPS-01

Skill: `$gps-release`, `$gps-auth`

Acceptance:
- Per-route rates, auth brute-force, CSP/headers, bounded WS/pool/query timeouts dan metrics.
- No sensitive logs/labels, CSRF/replay and policy regression, CI dependency checks terverifikasi.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [OPS-02.md](../../docs/execution/evidence/OPS-02.md)

### OPS-03 — Backup/restore dan rollback drill

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: OPS-02

Skill: `$gps-release`

Acceptance:
- pgBackRest full/incremental/WAL, encryption/keys runbook dan isolated restore.
- Measured RPO/RTO, tombstone/expiry purge, dan invalidasi auth tersedia; runbook menetapkan syarat kompatibilitas rollback.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [OPS-03.md](../../docs/execution/evidence/OPS-03.md)

### OPS-04 — Load dan E2E release readiness

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: OPS-03

Skill: `$gps-release`

Acceptance:
- Synthetic 100-device steady/burst/heartbeat/backfill, documented environment/seed/timings/storage forecast.
- Semua AC P0, auth/tenant/concurrency/portal tests lulus; audit evidence bukan hanya checklist.

Sumber: [gps-tracking-implementation-plan.md](../../docs/gps-tracking-implementation-plan.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [OPS-04.md](../../docs/execution/evidence/OPS-04.md)

### RET-01 — Konfigurasi retensi posisi deployment

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: OPS-01

Skill: `$gps-storage`, `$gps-release`, `$gps-backlog`

Acceptance:
- POSITION_RETENTION_DAYS tervalidasi 1–36500, default 365, dan digunakan ingestion serta retention worker.
- Expiry baru memakai konfigurasi; penurunan konfigurasi membuat worker purge row lama tanpa membuka akses payload lintas tenant.
- Fresh/upgrade migration, default kalender, nilai 60 hari, purge/RLS dan regression tests lulus; dokumentasi .env dan operasi menjelaskan rollout.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md), [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md), [DECISIONS.md](../../docs/execution/DECISIONS.md)

Evidence: [RET-01.md](../../docs/execution/evidence/RET-01.md)

### TENANT-01 — Tutup akses admin lintas-tenant pada resource dan realtime

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: DB-03, ORG-03, LIVE-03, PORTAL-03

Skill: `$gps-storage`, `$gps-auth`, `$gps-tracking`, `$gps-portal`, `$gps-backlog`

Acceptance:
- Admin hanya mendapat izin untuk resource yang benar-benar dimiliki tenant yang dipilih; resource id tenant lain ditolak oleh REST dan WebSocket.
- Daftar target/device tetap terikat tenant meskipun koneksi database memiliki BYPASSRLS; konfigurasi development memakai login runtime least-privilege, bukan user superuser.
- Regression tests membuktikan daftar, location endpoint dan subscription tidak melintasi tenant; portal Histori dan Live berfungsi pada seed dua organisasi.

Sumber: [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md), [gps-tracking-management-api.md](../../docs/gps-tracking-management-api.md), [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md), [development-environment.md](../../docs/execution/development-environment.md)

Evidence: [TENANT-01.md](../../docs/execution/evidence/TENANT-01.md)

## P11 — Packaging dan pilot MVP

### PILOT-01 — Image dan deployment preflight

Status: **done** · Scope: mvp · Owner: Codex

Dependensi: OPS-04

Skill: `$gps-release`

Acceptance:
- Multi-stage image static/Rust, Caddy TLS routing, non-root, readiness/migration preflight dan rollback guide.
- Local/staging smoke and domain/SMTP/backup/host prerequisites are verified without secret evidence. Map/history UI smoke and authenticated cross-organization staging checks may be deferred from this preflight gate only by explicit user direction; any deferred checks must remain clearly recorded as unverified and be reopened or tracked as follow-up if the user reports an issue.
- Sebelum pilot, uji image/build aplikasi sebelumnya terhadap schema hasil migration di staging: startup/readiness dan negative tenant-authorization smoke lulus; simpan digest kedua build dan hasilnya. Jika build sebelumnya belum pernah production, gunakan kandidat dari code state berurutan. Uji gagal/tidak tersedia berarti pilot tetap tertahan.

Sumber: [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md), [DECISIONS.md](../../docs/execution/DECISIONS.md), [staging-vps-github-cicd.md](../../docs/staging-vps-github-cicd.md)

Evidence: [PILOT-01.md](../../docs/execution/evidence/PILOT-01.md)

### PILOT-02 — Pilot deployment terotorisasi

Status: **blocked** · Scope: mvp · Owner: Codex

Dependensi: PILOT-01

Skill: `$gps-release`

Acceptance:
- Otorisasi deployment dan environment tersedia; real migration/health, tenant isolation dan provider connectivity smoke.
- Backup/WAL monitoring aktif, production-safe rollback path dan deployment evidence; jangan mark done dari local mock.

Sumber: [gps-tracking-implementation-plan.md](../../docs/gps-tracking-implementation-plan.md), [gps-tracking-database-operations.md](../../docs/gps-tracking-database-operations.md)

Evidence: [PILOT-02.md](../../docs/execution/evidence/PILOT-02.md)

### PILOT-03 — Acceptance MVP dan handover

Status: **todo** · Scope: mvp · Owner: —

Dependensi: PILOT-02

Skill: `$gps-release`, `$gps-backlog`

Acceptance:
- Review semua required task/evidence dan pilot behavior, residual issue list tidak menyembunyikan P0.
- Runbook/handover lengkap, STATUS menyatakan MVP hanya setelah semua required tasks done.

Sumber: [gps-tracking-implementation-plan.md](../../docs/gps-tracking-implementation-plan.md), [gps-tracking-baseline-final.md](../../docs/gps-tracking-baseline-final.md)

Evidence: belum tersedia

## POST — Sesudah MVP

### POST-01 — MQTT adapter

Status: **deferred** · Scope: post_mvp · Owner: —

Dependensi: tidak ada

Skill: `$gps-tracking`

Acceptance:
- Dirancang/dikerjakan hanya pada scope lanjutan yang diotorisasi.

Sumber: [gps-tracking-api-contract.md](../../docs/gps-tracking-api-contract.md)

Evidence: belum tersedia

### POST-02 — Hardware binary adapters

Status: **deferred** · Scope: post_mvp · Owner: —

Dependensi: tidak ada

Skill: `$gps-tracking`

Acceptance:
- Pilih model/firmware/fixtures sebelum klaim dukungan.

Sumber: [gps-tracking-app-spec.md](../../docs/gps-tracking-app-spec.md)

Evidence: belum tersedia

### POST-03 — Scale/WebAuthn/analytics

Status: **deferred** · Scope: post_mvp · Owner: —

Dependensi: tidak ada

Skill: `$gps-plan`

Acceptance:
- Perubahan scope melalui revisi desain; tidak termasuk acceptance MVP.

Sumber: [gps-tracking-technical-decisions.md](../../docs/gps-tracking-technical-decisions.md)

Evidence: belum tersedia
