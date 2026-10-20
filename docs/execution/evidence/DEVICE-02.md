# DEVICE-02 — Provisioning, rotasi dan disable device

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- `POST /v1/devices/{id}/credentials/rotate` hanya menerima portal session + CSRF/Origin sah, `devices:credentials:write`, serta recent auth lima menit dengan MFA aktif. Token `gps_live_…` acak 256-bit hanya muncul pada response sukses `Cache-Control: no-store`; DB menyimpan SHA-256 digest saja.
- Rotasi memegang organization shared lock lalu device row lock, mencabut token lama dan memasukkan token baru dalam satu transaksi. Unique partial index existing tetap menjadi invariant one-active-token. Device harus aktif dan memakai adapter MVP `http-json`.
- `GET /v1/devices/{id}/credentials` mengembalikan metadata pagination bertanda tangan saja; token/hash tidak masuk DTO. Endpoint mensyaratkan object permission `credentials:write` (yang memberi read menurut grant policy).
- `DELETE /v1/devices/{id}/credentials/{credential_id}` memakai recent auth + MFA, izin credential, dan device lock. Riwayat lokasi tidak disentuh.
- `PATCH /v1/devices/{id}` saat `enabled:false` mengunci device dan mencabut semua credential aktif atomik. `enabled:true` tidak mengaktifkan token lama. Perubahan `ingestion_policy=always` dibatasi admin organisasi.
- README server menjelaskan lifecycle token. Lock menggunakan `tracking_storage::lock_tracking_resources` dengan urutan organisasi → device yang juga ditetapkan sebagai lock ingestion.

## File

`apps/server/src/lib.rs`, `apps/server/README.md`, dan test integrasi `device02_credential_lifecycle_permissions_and_device_locking`.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --all -- --check` | Lulus |
| `cargo check --workspace` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 9 passed; integration tests yang perlu DB ditandai ignored dalam run umum |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server device02_credential_lifecycle_permissions_and_device_locking -- --ignored --test-threads=1` | Lulus: 1 integration test. Memeriksa recent MFA, operator tanpa grant ditolak, operator dengan `credentials:write` + MFA diizinkan, viewer ditolak, token digest lookup, metadata tidak berisi token, hanya satu active token, rotasi/revoke, lock tunggu terhadap simulasi jalur ingestion, disable bersamaan lock ingestion, re-enable tidak memulihkan token dan rotate saat disabled ditolak |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server device01_target_device_routes_enforce_grants_and_validation -- --ignored --test-threads=1` | Lulus: 1 regresi CRUD/tenant sebelumnya |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` | Lulus: 4 migration/RLS/storage regressions |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server org02_membership_grants_last_admin_and_operator_mfa -- --ignored --test-threads=1` | Lulus: 1 regresi grant dan MFA operator |
| `pnpm contracts:check` | Lulus: OpenAPI valid dan generated TypeScript client tidak drift |
| `git diff --check` | Lulus |

DB pengujian adalah container PostGIS sementara tanpa volume, memakai user/device/org synthetic dan role auth/app terpisah. Container dihentikan sesudah tes.

## Batas verifikasi

Belum ada endpoint ingestion dalam backlog sebelum P07. Karena itu concurrent ingestion dibuktikan dengan transaksi fixture yang mengambil lock helper yang sama, bukan request ingestion end-to-end. Implementasi ingestion wajib mengambil lock organisasi/device yang sama lalu memvalidasi ulang credential setelah memperoleh device lock agar request dengan token yang dicabut tidak berjalan setelah rotasi/disable commit. Tidak ada credential produksi yang dibuat atau dirotasi.
