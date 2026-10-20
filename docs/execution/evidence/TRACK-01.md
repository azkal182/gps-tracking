# TRACK-01 — Tracking session lifecycle

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Ditambahkan domain backend `tracking/` dan route `POST /v1/sessions`, `GET /v1/sessions`, `GET /v1/sessions/{session_id}`, serta `POST /v1/sessions/{session_id}/end`.
- Session start mewajibkan `Idempotency-Key` 1–128 karakter. Request hash dibuat dari body yang sudah dinormalisasi; replay dalam 24 jam mengembalikan resource dan HTTP 201 yang sama. Key sama dengan request berbeda mengembalikan 409. Lock advisory per tenant/principal/route/key menyerialkan concurrent retries. Replay memeriksa kembali permission objek dan bearer yang masih aktif sebelum mengembalikan response.
- Portal request memakai login session, Origin dan CSRF untuk mutasi, serta tenant membership/object grants. Application bearer memerlukan scope session dan tenant yang cocok; izin operasi juga menginterseksikan allowlist target/device dan permission owner terkini. Migration 011 menyediakan policy function sempit untuk scope session tanpa memaksa scope CRUD target/device tambahan.
- Start/end memakai lock order organisasi → device → target → session. Timestamp server dibuat sesudah resource lock; unique active indexes dan exclusion constraint existing menolak sesi aktif ganda dan interval device yang overlap. Start memeriksa device enabled. End mengatur waktu dari DB setelah lock, mempertahankan `ended_at` pertama pada retry, mendukung batas zero-duration, dan tidak memerlukan koneksi/perintah hardware.
- List mendukung filter target, device, status, `from`/`to`, limit dan HMAC-signed keyset cursor yang terikat ke tenant dan seluruh filter. Detail dan list tidak melintasi tenant/object grants. Create/end menulis metadata audit tanpa koordinat atau secret.
- API scope policy dijelaskan di database operations dan management API. Schema tracking session tidak diubah; migration baru hanya menambahkan authorization capability untuk session scopes application bearer.

## Requirement trace

Mencakup `FR-SES-01`, `FR-SES-02`, `FR-SES-03`, `FR-SES-04`, `FR-SES-06`, dan `FR-SES-07`, serta acceptance `AC-01`, `AC-02`, `AC-10`, dan serialisasi transaksi pada `AC-12`. Endpoint histori posisi sesi tetap task berikutnya.

## File

`apps/server/src/tracking/mod.rs`, `apps/server/src/tracking/sessions.rs`, `apps/server/src/lib.rs`, `apps/server/src/tests/tracking.rs`, `migrations/202610190011_application_token_session_permission.sql`, `crates/tracking-storage/tests/database_migrations.rs`, `docs/gps-tracking-database-operations.md`, dan `docs/gps-tracking-management-api.md`.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 9 passed; 9 server dan 4 database integration tests ditandai ignored pada run umum |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server track01_session_lifecycle_idempotency_authorization_and_concurrency -- --ignored --test-threads=1` | Lulus: 1 integration test. Memeriksa portal create/list/detail/end, mandatory key, replay, key/body conflict, replay authorization setelah perubahan membership, viewer deny, application token dengan scope session saja, allowlist/grant/revoke, signed cursor/tamper, concurrent start satu device, concurrent end, dan transaksi posisi yang bersaing dengan end pada lock order yang sama |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` | Lulus: 4 integration tests. Termasuk migration fresh/upgrade, RLS, active-session uniqueness, exclusion untuk overlap/adjacent intervals dan zero-duration `[start,end)` yang kosong |
| `pnpm contracts:check` | Lulus: OpenAPI lint/validasi dan TypeScript client generation stabil |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tes tracker |

Tes database memakai container PostGIS localhost disposable tanpa volume, menjalankan data sintetik dan menghapus container setelah tes. Tidak ada database atau credential produksi yang disentuh.

## Batas verifikasi

Endpoint ingestion belum tersedia dalam task TRACK-01. Karena itu AC-12 diuji dengan transaksi fixture yang mengambil lock helper yang sama, menulis posisi sintetik bertimestamp di dalam sesi, lalu membiarkan end handler berjalan bersamaan. Ini membuktikan serialisasi dan interval hasil pada storage boundary, bukan end-to-end request ingestion. Task ingestion wajib memakai lock order yang sama serta mengevaluasi interval session dari `recorded_at`.
