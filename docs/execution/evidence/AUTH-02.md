# AUTH-02 — TOTP enrollment dan recovery

Tanggal: 2026-10-19  
Status: selesai untuk kebijakan MFA yang dapat ditentukan sebelum skema grant organisasi tersedia.

## Hasil

- Menambahkan endpoint `POST /auth/mfa/verify`, `/auth/mfa/enrollment`, `/auth/mfa/enrollment/confirm`, `/auth/mfa/recover`, `/auth/reauthenticate`, `/auth/mfa/recovery-codes/rotate`, dan `DELETE /auth/mfa` sesuai operasi yang telah ada pada OpenAPI.
- TOTP RFC 6238 menggunakan HMAC-SHA1, 6 digit, step 30 detik dan toleransi ±1 step. Secret 160-bit dienkripsi AES-256-GCM; nonce acak 96-bit dan versi key disimpan bersama ciphertext. `MFA_ENCRYPTION_KEY` wajib saat server memakai database; key versi lama dapat disediakan sebagai `MFA_ENCRYPTION_KEY_<version>`. Tidak ada fallback penyimpanan plaintext.
- Step TOTP diterima dan challenge diperbarui dalam transaksi dengan row lock; challenge kedaluwarsa setelah lima menit dan tertutup setelah lima percobaan. Challenge bukan session aplikasi.
- Enrollment pending berlaku 10 menit dan tidak menimpa factor aktif sebelum konfirmasi. Aktivasi MFA mengeluarkan 10 recovery code acak dengan entropy 128-bit, hanya menyimpan SHA-256 digest. Recovery mengonsumsi satu code dan challenge secara atomik, mencabut semua session, lalu hanya memberi challenge re-enrollment. Konfirmasi pengganti mengganti factor dan kode lama, mencabut session, serta meminta login ulang.
- Re-authentication mencatat waktu hanya setelah password benar dan, bila MFA aktif/wajib, step TOTP baru yang belum pernah digunakan. Recovery-code rotation memerlukan re-auth kurang dari lima menit. MFA tidak dapat dinonaktifkan saat policy saat ini menyatakan wajib.
- Pending membership berubah menjadi active hanya setelah konfirmasi enrollment; integration test mencakup password login → restricted enrollment challenge → confirm → membership aktif dan sesi awal diterbitkan. Password-only login/re-auth tidak melewati MFA.

## Berkas

- `apps/server/src/lib.rs` — routes, challenge/session transitions, transaction guards, re-authentication, code rotation dan disable policy.
- `apps/server/src/mfa.rs` — AES-GCM, TOTP, Base32, recovery-code generator serta unit tests.
- `migrations/202610190006_mfa_membership_activation.sql` — fungsi SECURITY DEFINER dan capability owner sempit untuk mengaktifkan membership pending sesudah factor enabled.
- `crates/tracking-storage/tests/database_migrations.rs` — uji auth role tidak dapat mengubah membership langsung; fungsi hanya mengaktifkan akun dengan factor aktif.
- `Cargo.toml`, `apps/server/Cargo.toml`, `Cargo.lock` — dependency kriptografi versi terkunci.
- `.env.example`, `apps/server/README.md` — konfigurasi key MFA dan key version lama.
- `docs/execution/backlog.json`, `docs/execution/WORKLOG.md` — task state, evidence dan handoff.

## Verifikasi

Semua perintah dijalankan pada 2026-10-19. Database integration memakai container PostGIS disposable dan kredensial sintetik; container dan file env dibersihkan setelahnya.

| Perintah | Hasil |
|---|---|
| `cargo fmt --all --check` | lulus |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | lulus |
| `cargo test --workspace --offline` | lulus; 7 test server unit pass, 2 database integration dan 3 storage integration sengaja ignored tanpa URL eksplisit |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | 11 lulus |
| `GPS_TEST_DATABASE_URL=<disposable> cargo test -p server mfa_login_recovery_and_reenrollment_are_atomic_and_restricted --offline -- --ignored --test-threads=1` | 1 lulus: replay/concurrency TOTP dan recovery, challenge cap 5, challenge bukan session, password-only bypass ditolak, re-auth, rotasi codes, MFA wajib tidak dapat dinonaktifkan, re-enrollment mengganti factor hanya setelah konfirmasi |
| `GPS_TEST_DATABASE_URL=<disposable> cargo test -p server auth_login_csrf_timeout_logout_and_revocation_lifecycle --offline -- --ignored --test-threads=1` | 1 lulus sebagai regresi AUTH-01 |
| `GPS_TEST_DATABASE_URL=<disposable> cargo test -p tracking-storage --test database_migrations --offline -- --ignored --test-threads=1` | 4 lulus untuk migration/RLS/storage regression, termasuk capability MFA sempit |

Semua data uji menggunakan akun, organisasi, key, dan recovery code sintetik. Tidak ada nilai secret yang dicatat dalam evidence.

## Batas dan tindak lanjut

Kebijakan sebelum ORG-02 mengenali platform admin, org admin, `pending_mfa`, dan setiap factor MFA yang telah aktif. Grant `credentials:write` belum memiliki tabel/skema di repo, sehingga policy login untuk operator dengan grant tersebut belum dapat dievaluasi aman saat login tanpa tenant context. ORG-02 sekarang memuat tindak lanjut untuk memperluas boolean policy helper dan menguji grant aktif/revoke/downgrade. Jangan mengklaim seluruh kebijakan role MFA selesai sebelum tindak lanjut itu lulus.

Rate limiting lintas akun/IP untuk login/re-auth tetap mengikuti OPS-02. Operasional distribusi/backup/rotasi key tetap memerlukan secret manager deployment dan belum merupakan deployment production.
