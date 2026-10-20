# AUTH-01 — Password login, cookie sesi dan CSRF

Tanggal: 2026-10-19  
Status: selesai

## Implementasi

- Menambahkan endpoint `GET /v1/auth/csrf`, `POST /v1/auth/login`, `POST /v1/auth/logout`, dan `DELETE /v1/auth/sessions/{id}`. Origin dibandingkan exact dengan `PUBLIC_ORIGIN`; login memakai pre-auth synchronizer token dari cookie + `X-CSRF-Token`. Semua respons auth dipaksa `Cache-Control: no-store`.
- Menetapkan hash password baru sebagai Argon2id v=19, `m=65536,t=3,p=1`, salt unik. Verifikasi berjalan di `spawn_blocking` dengan maksimal empat hash bersamaan; slot penuh menghasilkan 429. Unknown account tetap melalui hash dummy dengan cost yang sama. Password di luar 15–128 karakter ditolak. Email canonical dicari dengan trim + lowercase.
- Sesi menggunakan opaque random 256-bit secret; hanya SHA-256 digest yang disimpan. Cookie `__Host-tracking_session` memiliki Secure, HttpOnly, SameSite=Lax, Path=/, Max-Age 12 jam, tanpa Domain. CSRF synchronizer token dirotasi oleh `GET /auth/csrf` dan hash sesi disimpan di database.
- Helper `authenticate_cookie_session` menolak sesi revoked, user nonaktif/auth-version stale, idle >=30 menit, dan absolute expiry; interactive touch dibatasi sekali per menit. Logout memerlukan Origin+CSRF, mencabut sesi dan idempoten. Session revoke hanya menerima sesi milik user yang sedang login dan sesi aktif.
- Login mengembalikan error generik identik untuk principal yang tidak dikenal dan password yang salah. Kebijakan MFA mandatory sebelum tenant context diambil lewat fungsi boolean `gps_tracking_user_requires_mfa`; migration 005 membuat role pemilik sempit, `BYPASSRLS` yang hanya memiliki kolom policy minimum dan tidak mempunyai akses tabel posisi. Akun yang belum memiliki faktor mendapat challenge enrollment 5 menit dengan cookie terpisah; challenge tidak diterima sebagai session cookie.
- Migration `202610190005_auth_mfa_policy.sql` melengkapi DB-03 tanpa mengubah role `gps_tracking_auth` menjadi BYPASSRLS. `apps/server/README.md` mendokumentasikan `DATABASE_URL`, `PUBLIC_ORIGIN`, dan capability policy.

## Verifikasi

| Pemeriksaan | Hasil |
|---|---|
| `cargo fmt --check` | lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | lulus |
| `cargo test --workspace` | lulus; 5 server unit tests, suite PostgreSQL ditandai ignored secara default |
| `GPS_TEST_DATABASE_URL=postgres://tracker_test:<redacted>@127.0.0.1:54395/tracker_test?sslmode=disable cargo test -p server auth_login_csrf_timeout_logout_and_revocation_lifecycle -- --ignored --test-threads=1` | 1 passed pada PostGIS disposable bersih |
| `GPS_TEST_DATABASE_URL=postgres://tracker_test:<redacted>@127.0.0.1:54395/tracker_test?sslmode=disable cargo test -p tracking-storage --test database_migrations -- --ignored --test-threads=1` | 3 passed pada PostGIS disposable bersih |
| Pemeriksaan logging auth | tidak ada tracing/logging macro pada `apps/server/src/lib.rs`; tes membandingkan body error kredensial dan memastikan password/session secret tidak ada pada JSON login |
| Cleanup | container `gps-auth01-codex` dan env sementara dihapus |

Tes integrasi menggunakan data sintetis: Origin mismatch/CSRF hilang, principal tidak dikenal, password salah, login berhasil, cookie+cache flags, hash sesi at rest, Origin+CSRF login, session absolute/idle timeout, logout termasuk pengulangan, session revoke, challenge MFA enrollment 5 menit, attempt_count awal, batas database menolak nilai >5, challenge tidak bisa dipakai sebagai sesi, MFA policy dapat diperiksa oleh `gps_tracking_auth`, dan role tersebut tidak dapat membaca tabel posisi. Suite storage mengulang migration upgrade, RLS/pool reuse, batas role serta domain/concurrency/retention DB-01 hingga DB-03.

## Batas dan tindak lanjut

- Endpoint verifikasi TOTP/enrollment dan konsumsi/increment attempt challenge berada pada AUTH-02; saat ini challenge telah dibatasi TTL, cookie dan constraint storage maksimum lima percobaan, tetapi belum dapat diselesaikan end-to-end sampai verifikator ada.
- Mandatory MFA untuk operator yang memiliki grant `credentials:write` perlu ditambahkan ke fungsi policy saat P05 menyediakan tabel/grant service. Saat ini policy mencakup platform admin, org admin aktif, membership pending MFA, dan MFA yang telah diaktifkan.
- Login limiter 5/account dan 30/IP per 15 menit, termasuk trusted-proxy IP parsing, tetap scope OPS-02. Hash concurrency tetap dibatasi di AUTH-01.
- Runtime test berjalan dengan principal database disposable lokal. Pemetaan login principal production dan secret provisioning belum dibuat atau diuji.
- API error envelope/request-id global belum dipasang oleh foundation; AUTH-01 menjaga status dan `no-store`, dan tidak merekam body/cookie.

## File

- `Cargo.toml`, `Cargo.lock`
- `apps/server/Cargo.toml`, `apps/server/src/lib.rs`, `apps/server/src/main.rs`, `apps/server/README.md`
- `migrations/202610190005_auth_mfa_policy.sql`
- `crates/tracking-storage/tests/database_migrations.rs`
- `docs/execution/evidence/AUTH-01.md`, `docs/execution/WORKLOG.md`, `docs/execution/backlog.json` dan generated views

## Follow-up regresi browser development — 2026-10-20

Keluhan `CSRF token tidak tersedia` ditelusuri ke `GET /v1/auth/csrf`: browser same-origin dapat menghilangkan header `Origin` pada GET, sedangkan endpoint sebelumnya hanya menerima header tersebut. Bootstrap CSRF kini menerima fallback `Referer` dengan scheme dan authority yang harus sama persis dengan `PUBLIC_ORIGIN`. Endpoint ini hanya GET; semua route auth yang mengubah state tetap wajib memakai exact `Origin`.

Verifikasi: `cargo fmt --check`; `cargo test --locked -p server csrf_bootstrap_accepts_same_origin_referer_when_get_omits_origin` (1 lulus); `cargo test --locked -p server tests::unit` (11 lulus); `cargo clippy --locked --workspace --all-targets -- -D warnings` (lulus). Smoke test melalui proxy Vite dengan header `Referer: http://localhost:5173/login` memperoleh HTTP 200 dan `Cache-Control: no-store`; body token tidak ditampilkan/disimpan. Backend development dimulai ulang dengan perbaikan. Login manual tetap perlu dicoba pemilik akun pada portal.

## Follow-up 403 setelah enrollment MFA — 2026-10-20

Login request mengembalikan 403 hanya jika pemeriksaan exact `Origin` atau cookie/header pre-auth CSRF gagal. Setelah enrollment, browser dapat masih memiliki sesi sementara melakukan request login; endpoint `GET /auth/csrf` sebelumnya merotasi hash CSRF sesi tetapi tidak memperbarui cookie pre-auth yang diperiksa POST login. Respons CSRF pada cabang sesi kini turut menyetel cookie pre-auth berumur lima menit ke token yang sama. Ini mempertahankan pasangan cookie/header untuk login ulang dan sesi yang sedang aktif.

Tes regresi integrasi `auth_login_csrf_timeout_logout_and_revocation_lifecycle` diperluas agar memastikan cookie dan body token sinkron serta login ulang tidak ditolak 403 setelah refresh CSRF sesi. Suite integrasi membutuhkan `GPS_TEST_DATABASE_URL` disposable dan tidak dijalankan di environment ini; test berhasil dikompilasi dengan `cargo test --locked -p server auth_login_csrf_timeout_logout_and_revocation_lifecycle --no-run`. `cargo fmt --check`, 11 server unit tests, serta workspace Clippy lulus. Backend dev dijalankan ulang; login manual pengguna masih diperlukan untuk memastikan alur browser.

## Follow-up sesi setelah verifikasi MFA — 2026-10-20

Portal memuat sesi baru dengan `GET /auth/me` setelah TOTP diterima, tetapi handler/route itu belum tersedia pada server meski sudah ada di OpenAPI dan dipanggil portal. Akibatnya verifikasi dapat menerbitkan cookie sesi lalu pemuatan profil gagal sebagai “Sesi tidak tersedia”. Ditambahkan handler yang hanya menerima sesi cookie valid dan origin same-origin, mengembalikan profil publik, flag MFA, serta membership akun sendiri. Membership lintas tenant diperoleh melalui function `SECURITY DEFINER` sempit `gps_tracking_list_user_memberships`; auth principal tidak diberi akses lokasi dan user ID selalu berasal dari sesi tervalidasi. Bootstrap GET menerima `Referer` exact-origin sebagai fallback saat browser tidak mengirim `Origin`.

Migration baru `202610200003_auth_me_memberships.sql`. Tes `mfa_login_recovery_and_reenrollment_are_atomic_and_restricted` sekarang mencakup login -> TOTP -> session cookie -> `GET /auth/me` dan lolos pada PostGIS sementara fresh (1 passed). Dalam proses, SQLx perlu dipaksa mengompilasi ulang daftar migration setelah file baru ditambahkan; marker build dependency ditambahkan di `crates/tracking-storage/src/lib.rs`. Migration terverifikasi tercatat pada database dev dan function mengembalikan 2 membership sintetis. Format, 11 server unit tests, workspace Clippy dan readiness lulus. Backend lokal sudah dimulai ulang.

## Follow-up 401 setelah token TOTP — 2026-10-20

Browser mengirim cookie `__Host-tracking_session` lama ke bootstrap CSRF ketika berada di halaman login. Jika sesi tersebut expired, idle-timeout, revoked, atau auth-version tidak lagi cocok, handler sebelumnya mengembalikan 401 sehingga klien tidak dapat memperoleh token pre-login. `GET /auth/csrf` kini memperlakukan cookie sesi yang tidak valid sebagai anonim, menerbitkan token/cookie pre-auth baru, dan menghapus cookie sesi usang (`Max-Age=0`). Sesi valid tetap menggunakan cabang rotasi CSRF sesi.

Verifikasi aktual: `cargo fmt --check`; `cargo test --locked -p server` (19 lulus, 15 integration tests ignored by default); `cargo clippy --locked --workspace --all-targets -- -D warnings`; integration `auth_login_csrf_timeout_logout_and_revocation_lifecycle` lulus pada database PostGIS disposable baru (1 passed); smoke test GET melalui proxy `localhost:5173/v1/auth/csrf` dengan cookie sesi sintetis-invalid menghasilkan HTTP 200, cookie pre-auth 300 detik dan cookie sesi `Max-Age=0`; `/readyz` HTTP 200. Database disposable dihapus setelah tes. Nilai token/cookie tidak ditampilkan.

Backend development lokal dimulai ulang memakai binary yang diperbarui. Pemilik akun perlu klik **Lanjutkan** lagi; jika alert sebelumnya menetap, muat ulang halaman login dan isi kredensial kembali.

File follow-up: `apps/server/src/auth_routes.rs`, `apps/server/src/tests/auth.rs`, `docs/execution/evidence/AUTH-01.md`, `docs/execution/WORKLOG.md`, `docs/execution/backlog.json` dan generated views.

## Follow-up audit semua halaman portal — 2026-10-20

403 lintas halaman berasal dari `session_user`: helper yang dipakai handler read-only mensyaratkan exact `Origin`, padahal browser dapat menghilangkan header itu untuk same-origin GET. Helper kini menerima fallback `Referer` hanya bila scheme dan authority sama persis. Semua mutasi tetap melalui `session_user_csrf` atau `require_pre_auth` dan tetap mensyaratkan exact `Origin` + CSRF.

Audit juga menemukan `GET /auth/sessions` tercantum di OpenAPI dan dipakai halaman portal tetapi belum memiliki route server. Ditambahkan daftar sesi milik user aktif saja, pagination cursor bertanda tangan yang diikat ke user, penanda sesi saat ini, dan label aman generik `Browser session` karena skema tidak menyimpan user-agent. Payload `/auth/me` kini memetakan role DB `organization_admin` ke role kontrak `admin`.

Verifikasi: `cargo fmt --check`; `cargo test --locked -p server` (19 passed, 15 ignored by default); `cargo clippy --locked --workspace --all-targets -- -D warnings`; tes integrasi `auth_login_csrf_timeout_logout_and_revocation_lifecycle` pada PostGIS disposable (1 passed; termasuk GET `/organizations` dengan Referer same-origin tanpa Origin, Referer lintas origin tetap 403, daftar sesi GET berisi sesi saat ini, serta role `/auth/me` sesuai kontrak); portal `typecheck`; portal Vitest (8 file, 31 passed); `project_tracker.py validate/render`.

Pemeriksaan browser Chrome setelah backend diperbarui: Ringkasan, Organisasi, dan Devices menampilkan data; Sesi login kini menampilkan sesi browser saat ini. Live menampilkan 5 target, 2 sesi aktif dan 2 target berposisi melalui REST, tetapi WebSocket melaporkan subscription ditolak. Histori masih menampilkan `Respons histori tidak cocok dengan organisasi aktif.` sehingga sumber device belum tersedia. Peta juga belum menampilkan tile karena `VITE_MAP_STYLE_URL`/`VITE_MAPTILER_KEY` belum dikonfigurasi. Temuan live/history bukan lagi pola 403 Origin dan perlu investigasi otorisasi/tenant terpisah; jangan melemahkan pemeriksaan tenant untuk menutupi sentinel tersebut.

Backend development dijalankan ulang. Browser tab dibiarkan pada halaman Histori untuk kelanjutan diagnosis.
