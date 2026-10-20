# ORG-02 — Membership, grant dan peran

Status: **DONE** — 2026-10-19

## Implementasi

- Menambahkan endpoint management untuk daftar membership, perubahan/perberhentian membership, dan baca/penggantian object grants sesuai kontrak OpenAPI yang sudah ada. Daftar membership memakai cursor HMAC terikat tenant, limit default 50/maksimum 100, dan hanya mengembalikan metadata akun yang diperlukan.
- Menambahkan migration `202610190009_membership_grants.sql`: `object_grants` dengan FK tenant-komposit, constraint tipe/resource/permission, unique constraint, RLS `ENABLE` dan `FORCE`, serta indeks untuk lookup membership/resource.
- Mengunci mutasi membership/grant di fungsi database `SECURITY DEFINER` yang sempit. Role auth tidak dapat menulis membership atau membaca grants secara langsung. Perubahan memerlukan admin aktif, MFA dan re-auth baru; operasi mengunci organisasi untuk melindungi admin terakhir. Promosi admin tanpa MFA menjadi `pending_mfa`; suspend/remove menghapus grants, mencabut token milik anggota dan merevoke sesi terkait.
- Menambahkan helper izin tenant/object dan izin pemilik application token. Akses token beririsan dengan scope token, allowlist resource, status membership dan grants pemilik yang terbaru. Grant creator memberikan read/write pada resource baru.
- Kebijakan login MFA kini juga mewajibkan MFA bagi operator aktif dengan `credentials:write`, melalui hasil boolean tanpa membaca koordinat. Pemeriksaan deferred allowlist application token dijalankan sebagai policy owner terbatas agar operasi revoke oleh role auth tidak memerlukan akses langsung ke tabel token.
- Menguji pagination dan minimisasi metadata, grant creator, perubahan grant, penolakan resource lintas tenant, viewer read-only, izin application-token owner dan perubahan grant live, pencabutan token saat remove, downgrade credentials permission, operator MFA, promosi pending-MFA dan dua perubahan admin bersamaan.

## Verifikasi

Fixture database memakai PostgreSQL/PostGIS lokal disposable; akun, tenant, resource dan token sintetis.

| Command/check | Hasil |
|---|---|
| `cargo fmt --all --check` | Lulus |
| `cargo clippy -p server --all-targets --offline -- -D warnings` | Lulus |
| `GPS_TEST_DATABASE_URL=<PostgreSQL disposable lokal> cargo test -p server org02_membership_grants_last_admin_and_operator_mfa -- --ignored --test-threads=1` | Lulus: 1 integration test; migrasi diterapkan pada database fresh |
| `GPS_TEST_DATABASE_URL=<PostgreSQL disposable lokal> cargo test -p tracking-storage fresh_upgrade_uniqueness_missing_tenant_and_atomic_consumption -- --ignored --test-threads=1` | Lulus: 1 integration test; memeriksa penerapan migrasi dan privilege/RLS ORG-02 |
| `cargo test --workspace --offline` | Lulus: 9 unit tests; 9 integration suites di-ignore karena run umum tidak diberi URL disposable |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tests |
| `pnpm contracts:check` | Lulus: Redocly lint, TypeScript generation dan generated drift check |
| `git diff --check` | Lulus |

## Batas verifikasi

- Pengujian mencakup database disposable lokal, tidak mencakup deployment atau database produksi.
- Endpoint baru menggunakan kontrak yang telah tersedia; tidak ada perubahan kontrak yang diperlukan.

## Langkah berikut

Lanjutkan ke task dependency-ready berikutnya: DEVICE-01, registrasi target/device dan lifecycle credential.
