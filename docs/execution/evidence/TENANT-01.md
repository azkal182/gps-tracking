# TENANT-01 — Batas tenant untuk admin, Histori dan realtime

Tanggal: 2026-10-20
Status: selesai

## Temuan dan perubahan

- Akar mismatch Histori: backend development memakai user PostgreSQL `tracker_dev`, yang merupakan superuser dan melewati RLS. Query daftar target/device bergantung pada RLS tanpa filter `tenant_id` eksplisit sehingga mengembalikan resource organisasi lain.
- Akar penolakan subscription Live: helper izin membership menganggap admin berhak atas semua ID resource tanpa memverifikasi bahwa target/device tersebut milik tenant yang diminta. Karena itu endpoint lokasi mengembalikan snapshot inactive untuk target foreign; portal memasukkannya ke subscription dan WebSocket menolaknya dengan benar.
- Pemeriksaan izin kini membuktikan eksistensi dan kepemilikan resource di tenant sebelum mengevaluasi admin/grant. Daftar target dan device menambahkan predicate tenant eksplisit sebagai defense-in-depth bila konfigurasi DB salah.
- WebSocket `principal_is_current` kini mengatur tenant context pada auth transaction, sesuai FORCE RLS. Worker status realtime menggunakan fungsi sempit `gps_tracking_active_tenants()` yang hanya mengeluarkan UUID tenant aktif sebelum membaca data tenant satu per satu dalam transaksi scoped.
- Development runtime dialihkan ke login `gps_dev_auth` (capability auth) dan `gps_dev_app` (capability tracking), keduanya non-superuser/non-BYPASSRLS. `tracker_dev` tetap hanya untuk migration/devseed. Script provisioning aman diulang dan menulis runtime URLs ke `.env` dengan mode 0600.

## Files

- `migrations/202610190015_admin_resource_tenant_boundaries.sql`
- `migrations/202610190016_realtime_tenant_enumeration.sql`
- `apps/server/src/resources.rs`
- `apps/server/src/tracking/realtime.rs`
- `apps/server/src/tests/devices.rs`
- `apps/server/src/tests/tracking.rs`
- `scripts/dev_db_roles.sh`
- `.env.example`
- `.env` (lokal, berisi kredensial runtime; nilainya tidak disalin ke evidence)
- `docs/execution/development-environment.md`

## Verifikasi

| Pemeriksaan | Hasil |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo test --locked -p server` | PASS: 19 lulus; 15 integration tests memerlukan DB dan tetap ignored pada run umum |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable tracker_tenant_test> cargo test --locked -p server device01_target_device_routes_enforce_grants_and_validation -- --ignored --test-threads=1` | PASS: 1. Memeriksa foreign target ditolak, list tetap pada tenant, termasuk ketika tracking pool memakai superuser/BYPASSRLS |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable tracker_tenant_test> cargo test --locked -p server live02_websocket_ticket_subscription_and_revoke -- --ignored --test-threads=1` | PASS: 1 dengan auth/app capability pools; termasuk ticket, subscribe, revocation dan transisi status konektivitas |
| `pnpm --filter @backend-tracker/portal test -- --run` | PASS: 31 tes pada 8 file |
| `pnpm --filter @backend-tracker/portal typecheck` | PASS |
| `bash scripts/dev_db_roles.sh` | PASS, idempotent; role auth/app non-superuser dan non-BYPASSRLS |
| Pemeriksaan Postgres scoped sebagai `gps_dev_app` | PASS: hanya 2 device tenant Logistics terlihat; izin target organisasi lain `false` |
| Browser `http://localhost:5173/history` | PASS: daftar hanya `Tracker Demo 05` dan `Tracker Demo 04`; mismatch tenant tidak muncul |
| Browser `http://localhost:5173/live` | PASS: “Realtime tersambung”; 2 target, 2 sesi aktif, 2 punya posisi; tidak ada penolakan subscription |
| `APP_ENV=development cargo run --locked -p server --bin devseed` | PASS: migration diterapkan; seed yang ada tidak diubah |
| `curl http://127.0.0.1:8080/healthz` | PASS: HTTP 200 setelah restart server dengan runtime DB roles |

## Batasan

- Database `tracker_tenant_test` adalah disposable lokal untuk integration tests. Browser menampilkan data seed sintetis saja.
- Login admin pada dua organisasi tetap memberi akses ke semua resource milik tenant yang sedang dipilih, sesuai model peran. Pemeriksaan tambahan mencegah resource ID tenant lain dianggap milik organisasi aktif.
