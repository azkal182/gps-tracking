# ORG-01 — Organisasi dan invitation lifecycle

Status: **DONE** — 2026-10-19

## Implementasi

- Menambahkan migration `202610190008_organization_lifecycle.sql` untuk privilege metadata organisasi/invitation yang sempit, fungsi `SECURITY DEFINER` dengan keluaran terbatas, konsumsi invitation atomik, dan pemeriksaan admin aktif dengan MFA. Role platform tidak memperoleh akses ke tabel posisi.
- Menambahkan konfigurasi `PLATFORM_DATABASE_URL` dengan pool yang secara eksplisit memakai role capability platform; role membership PostgreSQL saja tidak mengaktifkan atribut `BYPASSRLS`.
- Mengimplementasikan endpoint yang sudah dikontrak: daftar organisasi milik user, daftar metadata organisasi platform, create/suspend/reactivate organisasi, reissue undangan admin awal, metadata/update organisasi tenant, create/list/resend/revoke invitation, dan acceptance untuk akun baru atau user yang sudah login.
- Create organisasi dan undangan admin/outbox/audit bersifat atomik. Token disimpan sebagai digest, email payload dienkripsi, resend mengganti token lama, dan email outbox lama yang belum dikirim dibersihkan sebelum token pengganti diantrikan.
- Mengimplementasikan pagination default 50/max 100 dengan sort stabil dan cursor HMAC yang terikat ke user/tenant/list scope. Cursor palsu atau lintas scope ditolak.
- Suspend organisasi merevoke login sessions anggota; request tenant berikutnya ditolak. Admin baru tanpa MFA mendapat membership `pending_mfa`; akun baru harus login setelah acceptance. Akun existing harus cocok dengan email invitation.

## Verifikasi

Semua tes database memakai PostGIS disposable `org01_test` pada localhost port 54339; data dan akun uji sintetis.

| Command/check | Hasil |
|---|---|
| `cargo fmt --all --check` | Lulus |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | Lulus |
| `cargo test --workspace --offline` | Lulus: 9 unit tests; 4 DB suites berstatus ignored tanpa URL pada run umum |
| `cargo test -p server org01_platform_tenant_and_invitation_lifecycle -- --ignored --test-threads=1` dengan `GPS_TEST_DATABASE_URL` menunjuk ke PostgreSQL disposable lokal | Lulus: 1 integration test, termasuk isolation role, lifecycle, negative cases dan cursor pagination/tamper |
| `cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` dengan `GPS_TEST_DATABASE_URL` menunjuk ke PostgreSQL/PostGIS disposable lokal | Lulus: 4 integration tests untuk migration, capability, RLS, constraints dan concurrency |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tests |
| `pnpm contracts:check` | Lulus: Redocly OpenAPI lint, TypeScript generation dan drift check |
| `git diff --check` | Lulus |
| `python3 scripts/project_tracker.py validate` | Lulus: 39 tasks tervalidasi |

## Batas verifikasi

- SMTP delivery untuk template invitation belum diuji dengan Mailpit pada task ini; AUTH-03 sudah memverifikasi dispatcher/retry SMTP secara umum, sementara ORG-01 memverifikasi pembuatan encrypted outbox dan token lifecycle lewat database integration test.
- Tes tidak mengklaim deployment atau kesiapan produksi. `PLATFORM_DATABASE_URL` harus dikonfigurasi untuk mengaktifkan operasi platform.

## Langkah berikut

Lanjutkan ke ORG-02: membership, grants, proteksi admin terakhir, dan otorisasi object/resource.
