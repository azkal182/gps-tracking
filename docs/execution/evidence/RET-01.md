# RET-01 — Konfigurasi retensi posisi deployment

Status: **selesai** pada 2026-10-20.

## Perilaku

- `POSITION_RETENTION_DAYS` ditambahkan ke `.env.example`. Default 365; rentang valid 1–36.500 hari. Server menolak nilai malformed/out-of-range saat startup. `retentionctl` membaca nilai yang sama dan menolaknya sebelum koneksi jika tidak valid.
- Ingestion menyimpan expiry per posisi dari `recorded_at` dan konfigurasi. Nilai default 365 memakai UTC calendar-year semantics, termasuk 29 Februari → 28 Februari. Trigger migration mempertahankan expiry default 365 bagi penulis lama/test fixture yang tidak mengisi kolom.
- Retention worker memakai cutoff konfigurasi untuk menghapus row dengan expiry lama dari kebijakan terdahulu, dalam batch terbatas dan tetap melalui RLS capability. Nilai identik wajib diset pada proses HTTP dan scheduler worker. Setelah konfigurasi diturunkan, data yang sudah melewati cutoff dibersihkan pada siklus worker berikutnya (jadwal operasi per jam); sampai purge berhasil, API dapat tetap membaca row dengan expiry tersimpan lama.
- Masa simpan audit, logs, token, email outbox, dan idempotency tidak berubah.

## Perubahan

- Migration `202610200002_configurable_position_retention.sql` mengubah generated expiry menjadi expiry tersimpan, menambahkan helper UTC per-hari dan default trigger, constraint expiry, serta kebijakan retention RLS berdasarkan transaction-local cutoff.
- Server config/state, ingestion, CLI retention dan storage purge menerima konfigurasi. Dokumentasi operations, app spec, decision log EXEC-008, restore runbook, `.env.example`, backlog dan evidence diperbarui.

## Verifikasi

- `cargo fmt --all` — passed.
- `cargo check --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo test --workspace` — 18 passed; 15 server DB tests + 4 storage DB tests di-ignore secara default.
- Migration integration suite pada PostGIS disposable: `GPS_TEST_DATABASE_URL=... cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` — 4/4 passed (fresh/upgrade, capability/RLS, retention/concurrency).
- Regression terarah dengan database PostGIS disposable: `tenant_rls_roles_lookup_worker_and_pool_reuse` — passed; membuktikan row lebih tua dari 60 hari yang awalnya ber-expiry 365 hari dapat dipurge dengan capability worker, worker tidak dapat membaca kolom lokasi, dan row live tetap terlindungi.
- `tracking_domain_constraints_retention_and_concurrency` dijalankan dengan purge konfigurasi 60 hari — passed.
- Retention worker database dan volumenya dihapus setelah tes; tidak ada container lain yang dihapus.

## Catatan rollout

Atur `POSITION_RETENTION_DAYS=60` pada `.env`/environment server **dan** environment scheduler `retentionctl`, lalu restart keduanya. Tidak perlu edit kode untuk mengubah jumlah hari. Migration harus diterapkan terlebih dahulu. Jangan menurunkan hanya salah satu proses: ingestion akan menulis expiry baru menurut server, sedangkan worker menggunakan cutoff-nya sendiri.
