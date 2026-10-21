# PORTAL-04 — Portal manajemen organisasi, anggota dan grants

Tanggal: 2026-10-21
Status: selesai

## Hasil

Portal menambahkan halaman platform `/platform/organizations` dengan daftar organisasi ber-cursor, pembuatan organisasi dan undangan admin pertama, suspend/reactivate, serta reissue undangan admin awal untuk bootstrap yang belum memiliki admin aktif. Semua operasi sensitif meminta recent authentication. Halaman hanya menggunakan metadata organisasi; keputusan akses tetap ditentukan server.

`GET /auth/me` kini mengembalikan `platform_admin` untuk akun yang sedang login. Field ini hanya dipakai untuk navigasi/hint UI dan bukan otorisasi. OpenAPI serta dokumentasi management API diperbarui dan client TypeScript digenerate ulang.

Halaman organisasi kini mengelola anggota: ubah role/status, cabut membership, dan ganti grants eksplisit organisasi/target/device. Aksi memakai recent authentication, UI tidak menawarkan perubahan status pending MFA, dan proteksi last-admin serta tenant ownership tetap dilakukan backend. Query dan cache discope ke organisasi aktif.

## Evidence dan verifikasi

- `pnpm --dir apps/portal typecheck` — lulus.
- `pnpm --dir apps/portal lint` — lulus.
- `pnpm --dir apps/portal build` — lulus.
- `pnpm --dir apps/portal test` — lulus, 9 file / 41 test.
- Tes baru mencakup akses platform vs akun biasa, perubahan role dengan reauthentication, dan penggantian grant dengan reauthentication.
- `pnpm contracts:lint` — lulus; `pnpm contracts:generate` — lulus dan menghasilkan `AuthMeResponse.platform_admin`.
- `cargo fmt --all -- --check` dan `cargo test -p server --no-run` — lulus.
- `cargo test -p server` — 20 lulus termasuk test `devseed`; 15 test integrasi diabaikan karena `GPS_TEST_DATABASE_URL` disposable tidak tersedia.
- `python3 scripts/project_tracker.py validate` — lulus setelah backlog diperbarui.

## Batas verifikasi

Route dan perubahan user response backend dikompilasi, tetapi endpoint `auth/me` dan transisi database tidak diuji langsung karena database disposable tidak tersedia. Test portal memakai fixture API; tidak ada email nyata yang dikirim. Server tetap satu-satunya otoritas untuk platform access, last-admin rules, tenant isolation, dan recent-auth policy.
