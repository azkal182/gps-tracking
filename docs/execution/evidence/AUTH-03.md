# AUTH-03 — Email reset/aktivasi dan bootstrap CLI

Tanggal: 2026-10-19  
Status: selesai; acceptance task telah diverifikasi pada implementasi lokal/disposable.

## Perilaku yang diselesaikan

- `POST /v1/auth/password/forgot` memberi respons generik yang sama untuk email terdaftar dan tidak terdaftar. Permintaan memakai Origin dan pre-auth CSRF, rate limit gabungan per email ternormalisasi dan peer IP, serta mengembalikan `Retry-After` saat dibatasi.
- Link reset/aktivasi/recovery berumur terbatas dan one-use. Token hanya disimpan sebagai digest; payload email dienkripsi AES-GCM dengan key outbox terpisah dari key MFA. GET tidak mengonsumsi link.
- Reset mengganti Argon2id password, menaikkan `auth_version`, mencabut sesi dalam transaksi, dan mempertahankan MFA. Aktivasi menetapkan akun aktif tanpa membuat sesi.
- Operator recovery mewajibkan token otorisasi dari file mode 0600 serta bukti email yang terikat ke akun yang sama. Recovery mencabut sesi dan MFA lama lalu memberi challenge MFA terbatas; tidak membuat sesi penuh.
- `authctl` menyediakan bootstrap admin pertama dan penerbitan ulang link aktivasi/recovery. Tidak ada password/token yang dicetak ke terminal. SMTP dispatcher memakai principal database khusus, lease/claim, retry/backoff, dan menghapus outbox setelah berhasil dikirim.

## Files

- `apps/server/src/lib.rs`, `apps/server/src/email.rs`, `apps/server/src/main.rs`
- `apps/server/src/bin/authctl.rs`, `apps/server/Cargo.toml`, `Cargo.lock`
- `migrations/202610190007_auth_email_worker.sql`
- `crates/tracking-storage/tests/database_migrations.rs`
- `contracts/openapi.yaml`, `apps/portal/src/api/generated.ts`
- `.env.example`, `apps/server/README.md`
- `docs/gps-tracking-management-api.md`, `docs/gps-tracking-database-operations.md`
- `docs/execution/DECISIONS.md`

## Verifikasi

Semua pengujian menggunakan kredensial, akun, alamat email dan token sintetis pada PostgreSQL/PostGIS serta Mailpit lokal disposable. Nilai token/password tidak dicatat di evidence.

| Perintah / skenario | Hasil |
|---|---|
| `cargo fmt --all --check` | Lulus. |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | Lulus. |
| `cargo test --workspace --offline` | 9 unit tests lulus; 3 server DB tests dan 4 storage DB tests berstatus ignored karena perintah workspace tidak diberi URL database. DB tests terkait dijalankan terpisah di bawah. |
| `GPS_TEST_DATABASE_URL=<disposable> cargo test -p server auth03_reset_activation_and_operator_recovery_invariants -- --ignored` | 1 lulus. Menguji reset menjaga MFA dan mencabut sesi, replay/expiry ditolak, GET scanner tidak mengonsumsi link, aktivasi tidak login otomatis, recovery menghapus faktor lama dan hanya memberi challenge, replay ditolak, serta proof akun berbeda ditolak dengan konsumsi token tetap rollback. Fixture dapat dijalankan berulang memakai hash sesi acak. |
| `GPS_TEST_DATABASE_URL=<disposable> cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` | 4 lulus, mencakup upgrade/fresh migration, auth/mailer role privilege, RLS dan regression storage. |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | 11 lulus. |
| `pnpm contracts:check` | Lulus: Redocly valid, TypeScript client dihasilkan, pemeriksaan drift lulus. |
| Unit `forgot_password_rate_limits_recipient_and_ip_together` | Lulus: batas email dan IP ditegakkan bersama. |
| Mailpit/API E2E disposable | Lulus: bootstrap admin pending dan aktivasi satu kali; pemulihan operator memakai token file mode 0600 dan bukti email; GET activation tidak mengonsumsi link; forgot untuk akun dikenal/tidak dikenal memberi status/body generik sama; SMTP dimatikan menghasilkan status failed lalu worker mengirim ulang setelah Mailpit hidup kembali; outbox sukses dibersihkan. Pesan dan token uji dibersihkan setelah verifikasi. |
| Unit `outbox_payload_is_authenticated_encryption_and_carries_expiry` | Lulus: payload tak tersimpan sebagai plaintext dan ciphertext termodifikasi ditolak. |

## Batas verifikasi

SMTP eksternal/produksi, secret manager, pengiriman email nyata dan konfigurasi deployment tidak diuji. Mailpit lokal membuktikan alur SMTP dan retry, bukan deliverability provider eksternal. Rate limiter forgot-password saat ini in-memory per instance; untuk deployment multi-instance, limit lintas instance masih perlu dipindah ke store bersama sesuai task operasional sebelum trafik produksi.
