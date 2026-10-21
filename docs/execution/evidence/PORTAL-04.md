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

## Follow-up audit tombol dan undangan — 2026-10-22

Task dibuka kembali setelah uji portal pengguna menemukan undangan admin awal tidak dapat dikirim ulang dan tautan email undangan membuka 404. Audit menemukan router portal hanya menerima `/invite/accept`, sedangkan email dispatcher mengirim ke `/accept-invitation`; tombol `Kirim ulang undangan` pada form platform memakai Base UI button default (`type=button`) sehingga `onSubmit` tidak pernah dipanggil. Tombol reissue tenant sudah memanggil endpoint setelah recent auth. Sisa form submit pada halaman-halaman portal telah memakai `type=submit`, dan tombol navigasi/aksi lain memakai handler yang sesuai.

Perbaikan:
- Tambah route `/accept-invitation` sambil mempertahankan route lama; token fragment tetap dibaca lalu dihapus dari address bar.
- Beri `type="submit"` pada tombol reissue admin awal.
- Tampilkan error untuk aksi konfirmasi gagal dan pencabutan sesi login; tombol pencabutan menunjukkan status proses dan tidak lagi menelan kegagalan.
- Tambah tes route link email, submit/re-auth/reissue admin awal, error action umum, serta kegagalan pencabutan sesi.

Verifikasi aktual 2026-10-22:

| Perintah | Hasil |
|---|---|
| `pnpm --dir apps/portal test` | Lulus: 11 file, 46 tes |
| `pnpm --dir apps/portal typecheck` | Lulus |
| `pnpm --dir apps/portal lint` | Lulus |
| `git diff --check` | Lulus |
| Audit interaksi dan form JSX seluruh `apps/portal/src/**/*.tsx` | Tidak ditemukan tombol form submit lain tanpa `type=submit`; aksi mutasi live tidak diklik saat audit |

Commit `0ce2032` dipush ke `origin/main`; status workflow deploy staging belum diverifikasi pada evidence ini. Tautan undangan yang dibagikan pengguna harus dianggap tidak rahasia lagi; gunakan undangan baru setelah route diperbarui di staging. Email/undangan nyata tidak dikirim oleh tes.
