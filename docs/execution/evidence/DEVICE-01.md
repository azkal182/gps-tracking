# DEVICE-01 — CRUD terbatas target/device

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Menambahkan route portal-session `POST/GET /v1/targets`, `GET/PATCH /v1/targets/{id}`, dan operasi yang sama untuk `/devices`.
- Setiap mutasi mensyaratkan session cookie, Origin dan CSRF valid. Header organisasi diwajibkan. Pemeriksaan `gps_tracking_member_has_permission` memakai auth capability; data dibaca/ditulis di transaksi tracking dengan `app.tenant_id` lokal dan FORCE RLS.
- Pembuatan target/device membutuhkan grant organisasi `targets:create`/`devices:create` dan transaction atomik membuat read/write object grants bagi pembuat.
- List menerapkan object read grant, filter nama dan protocol, signed cursor yang terikat tenant/filter/sort. Detail dan update memeriksa grant; tenant RLS menutup pembacaan lintas tenant.
- Request menolak field tak dikenal, trim dan validasi nama, object metadata berukuran maksimal 16 KiB, `external_ref` dan identifier dengan batas kontrak. `external_ref` unik per tenant; device identifier unik per tenant/protocol. Konflik DB dipetakan menjadi 409. Device MVP hanya menerima adapter aktif `http-json`; protocol/identifier immutable pada PATCH. Device response tidak pernah mengandung credential.
- Menambah pool tracking opsional `TRACKING_DATABASE_URL` terpisah dari `DATABASE_URL`/auth capability; dokumentasi runtime diperbarui agar auth capability tidak diberi privilege tabel tracking.

## File

`apps/server/src/lib.rs`, `apps/server/src/main.rs`, `.env.example`, `apps/server/README.md`, dan test integrasi `device01_target_device_routes_enforce_grants_and_validation`.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --all -- --check` | Lulus |
| `cargo check --workspace` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: server 9 passed; 6 integration ignored tanpa URL DB eksplisit |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server device01_target_device_routes_enforce_grants_and_validation -- --ignored --test-threads=1` | Lulus: 1 test pada role auth dan app terpisah; create/list/detail/PATCH target/device, duplicate conflict, validation/unknown fields, creator grants, tanpa grant dan cross-tenant RLS |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` | Lulus: 4 test migration/RLS/storage regression |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server org02_membership_grants_last_admin_and_operator_mfa -- --ignored --test-threads=1` | Lulus: 1 test regresi grants/membership |
| `pnpm contracts:check` | Lulus: OpenAPI valid dan generated TypeScript client tidak drift |
| `git diff --check` | Lulus |

PostGIS berjalan sebagai container khusus sementara tanpa volume; container dihentikan setelah tes. Fixture sintetis saja.

## Batas cakupan

Handler management resource pada task ini menggunakan portal session + CSRF. Endpoint bearer application token belum diimplementasikan untuk CRUD management ini; helper policy untuk token tetap memeriksa scope, allowlist dan grant pemilik, tetapi bearer request adapter adalah pekerjaan tersendiri sebelum klien integrasi dapat menggunakan endpoint CRUD tersebut. Credential issuance/lifecycle dan disable yang mencabut credential juga di luar task DEVICE-01.
