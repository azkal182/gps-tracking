# Rencana implementasi MVP

> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.
> Keputusan produk/teknis ditetapkan, belum diimplementasikan. Referensi: [Technical Decisions](./gps-tracking-technical-decisions.md), [Management API](./gps-tracking-management-api.md), [Database & Operations](./gps-tracking-database-operations.md).

| Urutan | Pekerjaan konkret | Dependensi | Kriteria selesai |
|---|---|---|---|
| 1 | Scaffold Cargo workspace, React/Vite TS/Tailwind/shadcn, pnpm, CI, Compose lokal PG/PostGIS/Mailpit | Dokumen baseline | Lint/typecheck/build, health/readiness dan migration kosong dapat berjalan; lockfiles committed |
| 2 | Tulis `contracts/openapi.yaml`, JSON schema WS dan error, generate TS client | Kontrak tracking + management | Semua route P0 tercakup; lint/schema validate; generated diff check di CI; contoh sesuai schema |
| 3 | Migrasi identity/tenant/auth/domain/positions/audit/outbox, RLS, composite FK dan indexes | Database design | Migration fresh/upgrade, negative cross-tenant pooled connection, concurrency session constraints lulus |
| 4 | Bootstrap, SMTP invite/reset, Argon2id, cookie/CSRF, TOTP, recovery dan sessions | 2–3 | Password-stage/MFA tidak bypass, replay/expiry/recovery/last-admin tests, secret tidak bocor |
| 5 | Organisasi/membership/grants + portal login/onboarding/organisasi | 4 | Dua tenant, multi-tab context, grants revoked REST/WS; platform tidak mendapat lokasi |
| 6 | Target/device provisioning, disable/re-enable, token rotasi, application token | 5 | Secret sekali, token lama ditolak, disabled device menolak, izin operator/viewer teruji |
| 7 | Tracking sessions, ingestion batch, late-window/dedupe/future retry | 3,6 | AC tracking/concurrent start/end dan mixed batch atomicity lulus |
| 8 | Heartbeat, status terpisah, location/history REST, WS ticket/event/timeout | 7 | Device diam online; stale posisi terpisah; restart/reconnect/revocation snapshot benar |
| 9 | Portal management target/device/session, peta MapLibre/MapTiler, histori, audit | 5–8 | Semua AC-PORT dan auth acceptance, cursor/filter/timezone, partial map label dan fallback tile failure |
| 10 | Purge, backup/WAL, restore/tombstone, security limits/headers, observability | 3–9 | Restore drill, expiry/purge/leap-year, dependency review, E2E dan load target diukur |
| 11 | Packaging/rollout pilot | 10 | Secret/domain/SMTP/tile config tersedia, migrasi + rollback operasional, health, budget/storage dan restore evidence; baru buka pilot |

Portal boleh dibangun bertahap setelah kontrak tersedia; dependencies tidak mewajibkan seluruh backend selesai sebelum UI mulai. Jangan membuat mock sebagai pengganti API yang diklaim selesai. Tidak ada deployment/pembelian layanan dalam fase perencanaan ini.

## Release gates

- Tenant/object authorization di server dan DB, termasuk WS, login/recovery dan metadata list, lolos negative tests.
- Semua contoh dan response shape cocok dengan OpenAPI/WS schema; no stale `tracking_status=online`.
- Sesi login, tracking session dan token device tidak tertukar; enforcement MFA dan credential scope teruji.
- Retensi/calendar expiry, dedupe, expiry queries, backup restore, RPO/RTO memiliki bukti.
- Peta tidak menyembunyikan histori parsial; API tetap berfungsi saat tile provider gagal.
- Target kapasitas diuji pada hardware/fixture terdokumentasi, disk forecast cukup; bila gagal, optimasi/partitioning sebelum produksi.

## Input deployment yang diperlukan nanti

Domain/TLS, host/storage, SMTP credentials dan sender, key MapTiler/paket yang sesuai, encryption/backup keys, serta alamat operator untuk notifikasi. Development memakai localhost, Mailpit dan test fixtures. Input ini tidak mengubah keputusan desain dan tidak perlu ditanyakan sebelum implementasi lokal.

## Pelacakan tugas operasional

Rencana fase ini diuraikan menjadi ID task, dependencies, acceptance dan evidence pada [BACKLOG](./execution/BACKLOG.md), bersumber dari `docs/execution/backlog.json`. [STATUS](./execution/STATUS.md) membedakan P00 planning/tooling, P01–P11 implementasi/pilot wajib MVP, dan POST fase lanjutan. Gunakan [workflow agent](./execution/WORKFLOW.md) agar setiap kelanjutan kerja memperbarui status berdasarkan bukti nyata.
