# PORTAL-01 — Halaman target/device/token/session

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Route Devices memakai feature folder portal untuk daftar/buat/edit target, daftar/registrasi device, lifecycle enabled, credential rotation, dan mulai/akhiri tracking session.
- Form target/device/session memakai Zod + React Hook Form, typed OpenAPI client, CSRF, tenant header dan React Query invalidation. Registrasi device terpisah dari token dan session; default ingestion policy `session_only` dan mulai tracking session selalu eksplisit.
- Self-grants dibaca melalui `GET /organization/members/{user_id}/grants`. Active member hanya dapat membaca grants miliknya sendiri; organization admin tetap dapat membaca grants anggota lain. PUT grant tetap admin-only. UI mengatur create, edit, device write, credential rotation, dan session controls per grant, bukan hanya berdasarkan role.
- Rotasi credential meminta recent authentication (password + MFA code bila diminta). Plaintext token hanya berada di state lokal komponen, tidak masuk React Query cache atau browser storage, dan hilang saat ditutup atau organisasi berganti.
- UI menampilkan loading/error/empty states, server conflict (termasuk session conflict), device online enablement terpisah dari histori, serta konsekuensi disable. Tombol tetap hanya affordance: server memvalidasi semua operasi.
- Halaman provisioning lazy-loaded. Peta live dan halaman histori berada pada PORTAL-02/03; penerimaan P09 menguji alur AC-PORT-01 lintas tiga slice.

## Kontrak dan keputusan

Management API §3, Portal spec §6, OpenAPI dan generated client diperbarui. Migration `202610190014_member_self_grant_read.sql` memperluas fungsi grants dengan pemeriksaan self-or-admin, tenant membership, sesi, target membership dan expiry. Keputusan tercatat pada EXEC-005. Pembagian AC lintas PORTAL-01/02/03 tercatat pada EXEC-006.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --all -- --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 13 passed; 14 integration tests yang memerlukan database diabaikan pada run umum |
| `GPS_TEST_DATABASE_URL=postgres://postgres:<synthetic>@127.0.0.1:55439/postgres cargo test -p server org02_membership_grants_last_admin_and_operator_mfa -- --ignored --test-threads=1` | Lulus: self-grant diizinkan; non-admin membaca grant anggota lain ditolak; tenant tanpa membership aktif ditolak; regression last-admin/grant/MFA lulus. Tes memaksa rebuild `tracking-storage` agar snapshot SQLx memuat migration baru. |
| `GPS_TEST_DATABASE_URL=postgres://postgres:<synthetic>@127.0.0.1:55439/postgres cargo test -p server device02_credential_lifecycle_permissions_and_device_locking -- --ignored --test-threads=1` | Lulus: operator tanpa `credentials:write` ditolak, operator dengan grant + MFA diizinkan, viewer ditolak, lifecycle token/disable tetap valid. |
| `pnpm lint` | Lulus: ESLint + Prettier |
| `pnpm typecheck` | Lulus |
| `pnpm test` | Lulus: 13 tests pada 2 file, termasuk one-time token, viewer, konflik 409, dan kontrol operator berdasarkan object grants |
| `pnpm build` | Lulus; halaman provisioning menjadi chunk terpisah 17.10 kB |
| `pnpm contracts:check` | OpenAPI lint dan TypeScript client generation lulus. Workspace saat ini belum melacak file sumber di Git, sehingga bagian `git diff` pada drift guard tidak dapat membuktikan perbandingan terhadap baseline tracked. |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tests |
| `python3 scripts/project_tracker.py validate` | Lulus setelah update backlog |
| `python3 scripts/project_tracker.py render` | Lulus; STATUS.md dan BACKLOG.md dihasilkan dari backlog canonical |

Database tes memakai container PostGIS disposable tanpa volume dan fixture synthetic; container dihapus sesudah tes. Tidak ada credential atau data lokasi nyata digunakan.

## Batas verifikasi

Tes portal memakai synthetic API fixtures; belum ada browser E2E yang menghubungkan portal live dengan server. AC-PORT-01 live-map journey diselesaikan di PORTAL-02, sedangkan history journey/retention setelah disable diselesaikan di PORTAL-03. AC tersebut tetap belum dinyatakan selesai secara keseluruhan sampai kedua task itu lulus.
