# ORG-03 — Portal login/onboarding dan organisasi

Tanggal: 2026-10-19  
Status: selesai pada cakupan task; verifikasi dilakukan terhadap portal dan kontrak API memakai fetch mock, bukan server hidup.

## Hasil

Portal React mengintegrasikan login, tantangan TOTP/recovery, enrollment MFA, aktivasi akun, penerimaan undangan, forgot/reset password, daftar/revoke sesi, daftar organisasi dan anggota, organisasi aktif per tab, undangan dan lifecycle-nya, serta rename organisasi ke API management yang bertipe OpenAPI. Mutasi sensitif meminta re-authentication terbaru dengan password dan MFA opsional sesuai kontrak. Query cache dan kunci query tenant-scoped; pergantian tenant membersihkan cache tenant lama, sementara request yang telah dimulai mempertahankan header tenant asal. Pilihan tenant disimpan dalam sessionStorage per tab; token autentikasi dan recovery code tidak disimpan di browser storage. Invite/reset token hanya hidup di memori halaman dan parameter URL segera dihapus.

## Evidence dan pemeriksaan

- `pnpm typecheck` — lulus.
- `pnpm test` — lulus, 1 test file dan 9 test; mencakup alur auth/MFA tanpa token persistence, invite dengan re-authentication sebelum POST, sesi tenant per-tab, preservasi header tenant request yang terlambat, dan isolasi respons saat tenant berganti.
- `pnpm lint` — lulus (ESLint dan pemeriksaan Prettier).
- `pnpm build` — lulus (Vite production build).
- `pnpm contracts:check` — lulus (Redocly lint, regenerate OpenAPI TypeScript client, tidak ada generated drift).
- `git diff --check` — lulus.

## Batas verifikasi

Mutasi/query API portal diuji menggunakan mock terhadap operasi/path kontrak OpenAPI; integrasi browser dengan server berjalan, SMTP, database dan end-to-end multi-tab pada dua browser belum dijalankan sebagai bagian task ini. Tidak ada claim bahwa layanan eksternal atau production sudah diuji.
