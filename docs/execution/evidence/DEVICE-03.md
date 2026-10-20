# DEVICE-03 — Application tokens dan audit query

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Endpoint admin organisasi membuat, melihat metadata, dan mencabut application integration token. Penerbitan meminta portal session, CSRF/Origin, MFA aktif dan recent re-authentication; pemilik organisasi dan tenant diverifikasi kembali di transaksi.
- Token memakai rahasia acak 256-bit dengan prefix `gps_app_`; plaintext hanya dikembalikan satu kali pada response `Cache-Control: no-store`. Database menyimpan digest SHA-256, expiry 90 hari, scope yang diizinkan, serta allowlist device/target eksplisit. List hanya mengembalikan metadata; revoke idempoten dan tidak mengembalikan secret.
- Storage menyediakan `gps_tracking_lookup_application_token(BYTEA)` sebagai capability sempit untuk memetakan digest bearer ke token/tenant/pemilik/scope hanya ketika token, organisasi, user, dan membership pemilik masih aktif. Hash tidak dibaca langsung oleh application role. Pemanggil tetap wajib mengecek scope, allowlist dan grant pemilik saat ini pada setiap operasi.
- Audit query membatasi akses pada admin tenant, menyediakan filter dan signed keyset cursor, batas rentang waktu 31 hari, menulis event `audit.events.read`, dan tidak memuat lokasi atau credential secret. `audit_events.id` disajikan sebagai string desimal agar aman dari kehilangan presisi JavaScript; ketidaksesuaian UUID di kontrak lama dicatat pada `EXEC-004`.
- Mutasi dan query token/audit mengunci organisasi sebelum validasi admin agar suspend/membership mutation terserialisasi. Kontrak OpenAPI dan generated TypeScript diperbarui.

## File

`apps/server/src/integrations.rs`, `apps/server/src/lib.rs`, `apps/server/src/tests/devices.rs`, `crates/tracking-storage/src/lib.rs`, `migrations/202610190010_application_token_lookup.sql`, `contracts/openapi.yaml`, `apps/portal/src/api/generated.ts`, `docs/gps-tracking-database-operations.md`, dan `docs/execution/DECISIONS.md`.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 9 passed; 8 tes server dan 4 tes database yang membutuhkan DB ditandai ignored dalam run umum |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server device03_application_token_lifecycle_and_audit_authorization -- --ignored --test-threads=1` | Lulus: 1 integration test. Meliputi MFA/permission negatif, allowlist/scope validation, penerbitan dan hash-only storage, expiry, lookup app-role/digest, owner suspension, pagination/cursor binding, tenant audit filter, timestamp/rentang, redaction, revoke idempoten dan token yang dicabut |
| `pnpm contracts:check` | Lulus: OpenAPI lint/validasi dan generate TypeScript client tanpa drift |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tes tracker |

Tes integrasi menggunakan PostGIS disposable pada port localhost eksplisit tanpa volume; container dihapus otomatis sesudah tes. Data, organisasi, user, dan token yang dipakai bersifat sintetis.

## Batas verifikasi

Token lookup primitive telah siap untuk adapter tracking berikutnya; endpoint ingestion/session/realtime yang menerima application bearer berada di task P07/P08 dan belum diklaim selesai oleh DEVICE-03. Query audit menguji pembatasan tenant dan redaction pada endpoint admin, sementara event audit tidak memuat koordinat.
