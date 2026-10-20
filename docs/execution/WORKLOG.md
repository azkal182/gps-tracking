# Worklog proyek

## 2026-10-20 — Perbaikan token aktivasi portal untuk PILOT-01

Smoke test staging menemukan tautan email menggunakan `#token=...`, tetapi halaman aktivasi hanya membaca query string; form karena itu tidak mengirim request aktivasi. Portal kini membaca token fragment/query sekali lalu membersihkan URL. Regression test membuktikan token fragment digunakan pada POST dan tidak tertinggal di address bar. Prettier, lint, typecheck, dan portal tests (32) lulus. Menunggu deploy perbaikan dan verifikasi browser staging; evidence tambahan di [PILOT-01](./evidence/PILOT-01.md).

## 2026-10-20 — PILOT-01 preflight staging disiapkan; smoke blocked

Disiapkan multi-stage image Rust/portal+Caddy, Compose staging privat, SQLx one-shot migration, runtime DB roles terbatas, backup sebelum migrasi, worker retensi, rollback image tanpa rollback schema, dan panduan VPS/GitHub Actions. CI hanya mem-publish image dan deploy SHA saat push `main` lulus quality job. Rust format/check/clippy/tests, portal lint/typecheck/tests/build, dan shell syntax lulus. Portal image berhasil dibangun lokal; Compose dan Caddy lolos validasi. Dockerfile API lolos static check, tetapi image Rust penuh tidak dibangun karena ruang lokal tersisa sekitar 4 GiB; staging smoke belum dapat berjalan tanpa VPS/GitHub environment. Domain/VPS/SMTP/GitHub secrets juga belum dikonfigurasi; PILOT-01 blocked dan bukan klaim MVP live. Evidence: [PILOT-01](./evidence/PILOT-01.md).

## 2026-10-19 — PLAN-01 selesai

Baseline planning MVP v1.0 difinalkan pada sesi pengguna. Ini kemajuan perencanaan, bukan feature implementation. Evidence: [PLAN-01](./evidence/PLAN-01.md).

## 2026-10-19 — AGENT-01 dimulai

Menyiapkan AGENTS.md, delapan skill lokal .agents/skills, coverage seluruh planning, canonical backlog dan tracker/evidence workflow. Implementasi aplikasi tidak dimulai pada tugas ini. Validasi dan hasil akhir akan dicatat pada evidence AGENT-01.

## 2026-10-19 — AGENT-01 selesai

Delapan skill lokal lulus validator resmi, tracker lulus 11 tests, tautan/JSON/coverage planning diperiksa. Evidence: [AGENT-01](./evidence/AGENT-01.md). P00 selesai 2/2; implementasi MVP tetap 0/34. Next-ready FOUND-01. Tidak ada produk/deployment/commit yang diklaim selesai.

## 2026-10-19 — FOUND-01 dimulai

Memulai scaffold Cargo workspace dan server health sesuai baseline final. Hasil akhir dan batas verifikasi dicatat pada evidence FOUND-01.

## 2026-10-19 — FOUND-01 progress

Cargo workspace, server health/readiness, typed environment config, secret Debug redaction dan graceful shutdown sudah ditambahkan. `cargo fmt`, metadata, 11 tracker tests, serta tracker validation lulus. Rust build/clippy/test terblokir karena crates.io tidak dapat dijangkau dan dependency `axum` tidak ada di cache; FOUND-01 tetap `in_progress`, evidence: [FOUND-01](./evidence/FOUND-01.md).

## 2026-10-19 — FOUND-01 selesai

Workspace Rust dan server foundation selesai. Build, clippy (`-D warnings`), format check, dua server unit tests, Cargo metadata, 11 tracker tests, validasi dan render lulus. `Cargo.lock` dibuat setelah menjalankan Cargo dengan akses registry. Evidence: [FOUND-01](./evidence/FOUND-01.md). `/readyz` masih process-level sampai task storage menghubungkan pemeriksaan database.

## 2026-10-19 — FOUND-02 dimulai

Mulai scaffold portal React/Vite sesuai frontend stack final. Scope task dibatasi pada shell/tooling, route/loading/error, dan proxy API/WS; belum mengimplementasikan autentikasi atau fitur management.

## 2026-10-19 — FOUND-02 selesai

Portal React/Vite strict dan tooling pnpm selesai. Shell responsive, route/loading/error, komponen shadcn, query/form dependencies dan proxy HTTP/WS tersedia. Frozen install, typecheck, 4 tes UI, production build, lint/format, serta smoke proxy HTTP/WebSocket lulus. Evidence: [FOUND-02](./evidence/FOUND-02.md). Belum ada auth atau fitur management yang diklaim selesai.

## 2026-10-19 — FOUND-03 dimulai

Mengerjakan Compose disposable PostgreSQL/PostGIS dan Mailpit, sample environment tanpa secret, dokumentasi command, serta CI lint/typecheck/build/unit test sesuai backlog. Docker CLI belum tersedia di host; validasi runtime Compose akan dicoba setelah memeriksa opsi lingkungan, dan keterbatasan akan dicatat bila tetap tidak tersedia.

## 2026-10-19 — FOUND-03 selesai

Compose disposable PostgreSQL/PostGIS dan Mailpit, sample environment tanpa secret, panduan perintah lokal, serta workflow CI backend/frontend ditambahkan. Format, clippy, test Rust, pnpm frozen install, lint, typecheck, 4 test UI, build, Prettier, 11 test tracker, dan smoke test Compose/PostGIS/Mailpit lulus. Stack uji dibersihkan setelah smoke test. Hosted GitHub Actions dan migration/integration CI belum dijalankan; migration belum tersedia. Evidence: [FOUND-03](./evidence/FOUND-03.md).

## 2026-10-19 — CONTRACT-01 dimulai

Menyusun OpenAPI 3.1 untuk endpoint tracking REST §§4–7: target, device, session, ingestion posisi, heartbeat, authentication mode, tenant context, pagination, limits, dan error responses. WebSocket ticket/event dan endpoint management umum mengikuti CONTRACT-02.

## 2026-10-19 — CONTRACT-01 selesai

OpenAPI 3.1 mencakup 19 operasi tracking REST dan heartbeat, security mode, tenant, status terpisah, batas/pagination, idempotency, dan error envelope. Redocly lint dan validasi contoh, Prettier, frozen pnpm install, tracker validation, serta 11 test tracker lulus. Endpoint management umum dan WebSocket dilanjutkan pada CONTRACT-02; tes runtime menunggu implementasi backend. Evidence: [CONTRACT-01](./evidence/CONTRACT-01.md).

## 2026-10-19 — CONTRACT-02 dimulai

Memperluas kontrak OpenAPI dengan route auth/MFA/challenge, organisasi/platform, membership/invitation/grant, device credential/application token/audit. Melengkapi kontrak tiket WebSocket dan schema message/event berdasarkan API Contract §8 dan Management API §5. Runtime belum tersedia; bukti task akan memisahkan validasi schema dari perilaku server.

## 2026-10-19 — CONTRACT-02 selesai

OpenAPI kini mencakup seluruh route management/realtime yang direncanakan serta schema auth/MFA, grants, ticket, subscription, event, `observed_at` dan access rules. Redocly lint/example validation tanpa warning, Prettier, inspeksi 61 operasi, dan 11 test tracker lulus. Keputusan principal untuk mint ticket dicatat sebagai EXEC-002. Endpoint runtime dan authZ/concurrency tests tetap menunggu implementasi; evidence: [CONTRACT-02](./evidence/CONTRACT-02.md).

## 2026-10-19 — CONTRACT-03 dimulai

Mulai generate TypeScript client OpenAPI dan CI lint/example/generated drift checks. Menambahkan dependensi generator/client yang ditetapkan pada frontend stack; runtime API server belum tersedia.

## 2026-10-19 — CONTRACT-03 selesai

`openapi-typescript` menghasilkan `paths` dan schema types dari seluruh OpenAPI; portal kini memiliki typed `openapi-fetch` client dengan cookie credentials dan konteks organisasi/CSRF dinamis. `contracts:check` memvalidasi spesifikasi/contoh, regenerate client, dan mencegah generated diff pada CI. Contoh token aplikasi dipindah ke request contoh operasi dan constraint allowlist diungkapkan dengan JSON Schema `not` agar generation dan validasi contoh konsisten. Generator stabil dua kali; typecheck, lint/format, 4 tes portal, production build, 11 tracker tests, validasi dan render lulus. Endpoint runtime/authZ tetap belum diuji; evidence: [CONTRACT-03](./evidence/CONTRACT-03.md). Task berikutnya DB-01.

## 2026-10-19 — DB-01 dimulai

Mulai migrasi identity/tenant/auth pada `tracking-storage`; merujuk database operations dan management API. Docker CLI tidak ada di PATH shell, tetapi Docker Desktop binary tersedia lewat path aplikasi; pengujian akan memakai Compose project/volume disposable yang terpisah.

## 2026-10-19 — DB-01 selesai

SQLx/PostgreSQL migrations kini mencakup organisasi, user, membership/invitation, login sessions/challenges, MFA/recovery, account tokens dan encrypted email outbox. Penanganan konsumsi token/recovery atomik disediakan melalui helper transaksi. Tes PostgreSQL fresh → upgrade, migration rerun, tenant NOT NULL/FK, membership uniqueness dan race single-use lulus pada container PostGIS disposable; container/volume dibersihkan. Format, clippy `-D warnings`, workspace tests, 11 tracker tests, validate dan render juga lulus. RLS/FORCE RLS serta pool context menunggu DB-03; endpoint auth belum terhubung. Evidence: [DB-01](./evidence/DB-01.md). Berikutnya DB-02.

## 2026-10-19 — DB-02 dimulai

Mulai migrasi domain tracking/PostGIS, position expiry/dedupe, tenant-composite FK, constraint sesi, dan lock-order helper sesuai database operations. Pengujian diarahkan ke PostgreSQL/PostGIS disposable.

## 2026-10-19 — DB-02 selesai

Migrasi domain menambahkan target/device, credential, application token beserta allowlist resource tenant-scoped, session, positions, idempotency dan audit. Constraint/indeks tenant, interval, posisi dedupe dan expiry diterapkan; helper lock menormalisasi urutan organization → device → target → session. Tes PostGIS: 2 integration tests lulus, termasuk race single active session, exclusion interval, canonical expiry 29 Feb → 28 Feb, dedupe, tenant FK, dan lock list berurutan terbalik. `cargo fmt`, clippy `-D warnings`, workspace tests, 11 tracker tests dan validate lulus. RLS/tenant pool context dan repositories belum diklaim; itu scope DB-03. Evidence: [DB-02](./evidence/DB-02.md). Berikutnya DB-03.

## 2026-10-19 — DB-03 dimulai

Mengaktifkan RLS tenant-scoped dengan FORCE RLS dan role capability terpisah termasuk auth global, menambahkan tenant transaction helper, credential lookup `SECURITY DEFINER` terbatas, worker retensi, dan pemeriksaan isolation/pool reuse. Sebelum implementasi diperiksa ulang keputusan DB operations §2 serta skill `gps-storage`, `gps-auth`, `gps-backlog`, dan `gps-plan`.

## 2026-10-19 — DB-03 selesai

Migration 004 menerapkan FORCE RLS untuk 14 tabel tenant, owner schema non-bypass, application grants terbatas, device lookup melalui fungsi SECURITY DEFINER (digest mentah tidak dapat dibaca app), worker posisi/idempotency/audit yang hanya dapat menghapus data expired, principal platform-provisioning tanpa privilege lokasi, dan auth capability global non-bypass dengan membership tenant-scoped. Storage menyediakan transaksi yang selalu mengatur context tenant secara lokal. Tiga tes PostgreSQL/PostGIS lulus di DB disposable bersih, mencakup cross-tenant read/write/FK, missing context, owner RLS, pool reuse, credential lookup/status, worker scope dan provisioning scope. Format, Clippy, workspace tests, 11 tracker tests, validate dan render lulus. Runtime login-role provisioning, integrasi server/API, object-grants service authorization dan hosted CI belum diklaim. Evidence: [DB-03](./evidence/DB-03.md). Berikutnya AUTH-01.

## 2026-10-19 — AUTH-01 dimulai

Mengimplementasikan login password Argon2id, pre-auth/session CSRF, cookie session, challenge terbatas, session expiry/logout/revocation, dan pemeriksaan kebijakan MFA sesuai management API serta technical decisions. Diperiksa bahwa auth role hanya dapat membaca policy MFA melalui fungsi boolean sempit dan tetap tidak mendapat akses data posisi.

## 2026-10-19 — AUTH-01 selesai

Server menyediakan GET CSRF, password login generik, opaque server session dan cookie `__Host-`, logout idempoten, session revoke, validator idle 30 menit/absolute 12 jam, challenge cookie 5 menit dan penyimpanan token hanya berupa digest. Argon2id hashing memakai m=65536/t=3/p=1; verifikasi memakai bounded blocking capacity. Migration 005 menyediakan MFA policy lookup lintas organisasi secara boolean tanpa memberi auth role akses lokasi. `cargo fmt --check`, workspace clippy, workspace tests, satu integration test auth dan tiga integration tests PostGIS storage lulus. Tes mencakup wrong principal/password, CSRF/Origin, session hashes/cookie, absolute/idle timeout, logout/revoke, challenge expiry/attempt cap, policy role, dan penolakan tabel posisi. Challenge verifier/TOTP menjadi AUTH-02; operator credential-grant policy dan persistent login rate limits tetap pada P05/OPS-02. Evidence: [AUTH-01](./evidence/AUTH-01.md). Berikutnya AUTH-02.

## 2026-10-19 — AUTH-02 selesai

TOTP RFC 6238 kini memiliki secret terenkripsi AES-256-GCM, key versioned di konfigurasi luar database, replay-step transaction guard dan challenge MFA 5 menit/5 percobaan. Implementasi menambahkan enrollment pending 10 menit, aktivasi `pending_mfa` melalui migration capability sempit (auth role tidak dapat mengubah membership langsung), 10 recovery codes hashed, atomic recovery/session revoke, restricted re-enrollment, recent re-auth, rotasi recovery codes, serta penolakan disable MFA wajib. Unit, clippy/workspace, tracker dan 4 PostGIS storage plus 2 auth integration tests lulus; disposable container dibersihkan. Evidence: [AUTH-02](./evidence/AUTH-02.md). Ekstensi kebijakan login operator `credentials:write` dicatat sebagai acceptance ORG-02 karena grants belum tersedia di skema saat AUTH-02 dikerjakan. Next-ready AUTH-03.

## 2026-10-19 — AUTH-03 dimulai (parsial)

Ditambahkan handler forgot-password dengan respons generik, token 30 menit yang merevoke token terdahulu dan disimpan terenkripsi pada outbox, serta reset yang mengonsumsi token atomik, memperbarui password/auth version, mencabut sesi dan mempertahankan MFA. Format, test server dan test storage lolos; tes database berstatus ignored karena `GPS_TEST_DATABASE_URL` belum disiapkan pada giliran ini. AUTH-03 tetap `in_progress`: SMTP worker/retry, CLI bootstrap/operator recovery, rate-limit forgot-password serta verifikasi Mailpit/DB negatif masih belum dikerjakan. Evidence: [AUTH-03](./evidence/AUTH-03.md).

## 2026-10-19 — AUTH-03 selesai

Forgot-password/reset, aktivasi akun, SMTP encrypted-outbox dispatcher dan retry, CLI bootstrap admin/operator recovery, serta limit forgot-password selesai. Reset mempertahankan MFA dan merevoke sesi; aktivasi tidak otomatis login; operator recovery memerlukan token file terbatas dan bukti email untuk akun yang sama, lalu memaksa enrollment MFA kembali tanpa menerbitkan sesi penuh. Tes PostgreSQL AUTH-03 dan 4 migration/storage integration tests lulus; Mailpit E2E membuktikan aktivasi, generic response, retry SMTP serta recovery CLI. Workspace format/clippy/tests, 11 tracker tests, kontrak TypeScript, dan lint contracts lulus. SMTP provider produksi belum diuji; rate limit masih in-memory per instance dan dicatat sebagai batas sebelum deployment multi-instance. Evidence: [AUTH-03](./evidence/AUTH-03.md). Berikutnya pilih task dependency-ready dengan `python3 scripts/project_tracker.py next`.

## 2026-10-19 — ORG-01 selesai

Endpoint organisasi platform/tenant dan lifecycle undangan selesai dengan capability database yang tidak memberi platform akses lokasi, atomic invite acceptance, encrypted outbox, invalidasi sesi saat suspend, dan pagination cursor HMAC. Tes PostgreSQL ORG-01 mencakup create/reissue, tenant invite create/resend/revoke, akun baru pending MFA, matching/mismatch, replay, cross-tenant, suspend/reactivate dan cursor tamper; 4 tes storage PostgreSQL/PostGIS juga lulus. Workspace clippy/tests, 11 tracker tests, OpenAPI lint/generated drift check, dan validasi tracker lulus. SMTP Mailpit khusus template invitation belum dijalankan; evidence: [ORG-01](./evidence/ORG-01.md). Berikutnya ORG-02.

## 2026-10-19 — ORG-02 selesai

Membership dan object grants selesai sesuai kontrak final. Database menegakkan tenant FK/RLS/constraints; mutasi membership/grant memakai fungsi terbatas, melindungi admin terakhir secara serial, mencabut sesi/token, dan memaksa MFA bagi operator dengan `credentials:write`. Helper otorisasi juga mengevaluasi scope/allowlist application token bersama grant pemilik saat ini. Pengujian PostgreSQL disposable mencakup positive/negative tenant, viewer write, grant lifecycle, owner-token, MFA, promosi admin pending-MFA, dan perubahan admin bersamaan. Workspace fmt/clippy/test, tracker unit tests, migration integration test, contract check, dan diff check lulus. Evidence: [ORG-02](./evidence/ORG-02.md). Berikutnya ORG-03 sesuai urutan dependency-ready tracker.

## 2026-10-19 — ORG-03 selesai

Portal kini mengintegrasikan login, MFA/recovery, aktivasi/reset akun, undangan, sesi, organisasi/anggota dan organisasi aktif. Mutasi sensitif meminta re-authentication; pilihan tenant bersifat per-tab dan query/request tetap tenant-scoped saat switch/late response. Tidak ada token autentikasi yang dipersist ke browser storage. Typecheck, 9 portal tests, lint/format, production build, OpenAPI lint/generated drift check, dan `git diff --check` lulus. Test API memakai mock; server hidup, SMTP dan browser E2E belum diverifikasi. Evidence: [ORG-03](./evidence/ORG-03.md). Berikutnya dipilih dari dependency-ready backlog.

## 2026-10-19 — DEVICE-01 selesai

Target/device CRUD kini tersedia melalui portal session: create/list/detail/PATCH, filter query/protocol, signed cursor, validasi schema, conflict mapping, grant read/write pembuat, dan tenant-scoped RLS. Runtime memisahkan `DATABASE_URL` auth capability dari `TRACKING_DATABASE_URL` tracking capability; tidak memperluas akses tabel pada auth role. `cargo fmt`, check, clippy, workspace tests, integration DEVICE-01 pada PostGIS disposable, empat migration/storage regressions, ORG-02 regression, OpenAPI generated drift check dan `git diff --check` lulus. Management CRUD saat ini portal-session only; bearer application-token adapter serta credential lifecycle belum termasuk task. Evidence: [DEVICE-01](./evidence/DEVICE-01.md). Berikutnya ambil task dependency-ready dari tracker.

## 2026-10-19 — DEVICE-02 selesai

Endpoint rotasi credential, daftar metadata dan revoke ditambahkan. Rotasi memerlukan portal CSRF/Origin, recent MFA dan grant `credentials:write`; token 256-bit hanya tampil sekali pada response no-store, DB menyimpan digest dan transaksi menjaga satu token aktif. Disable mengunci organization/device lalu mencabut semua token aktif secara atomik; re-enable tidak memulihkan token. `ingestion_policy=always` hanya admin organisasi. Cargo fmt/check/clippy/workspace tests, integration DEVICE-02 di PostGIS disposable, regresi DEVICE-01, empat storage migration tests, ORG-02 grant/MFA integration test, OpenAPI drift dan diff check lulus. Ingestion belum ada; lock-order diuji dengan transaksi fixture menggunakan helper yang sama dan kebutuhan revalidasi token setelah lock dicatat untuk task ingestion. Evidence: [DEVICE-02](./evidence/DEVICE-02.md). Berikutnya dipilih oleh tracker.

## 2026-10-19 — REFAC-01 dimulai dan selesai

Sebelum melanjutkan task produk, server monolitik dipisah berdasarkan domain: `lib.rs` menjadi composition root, handler resources/device, organization dan auth masing-masing memiliki modul, serta test server dikelompokkan ke unit/auth/organizations/devices. `docs/execution/STRUCTURE.md` menetapkan pola backend dan portal untuk langkah lanjutan. Cargo format, clippy, workspace tests dan tracker unit tests lulus; DB tests tetap ignored karena tidak ada `GPS_TEST_DATABASE_URL`. Kontrak lint/generate lulus dan hash generated client stabil. DEVICE-01/02 tetap selesai; DEVICE-03 kembali menjadi next-ready. Evidence: [REFAC-01](./evidence/REFAC-01.md).

## 2026-10-19 — DEVICE-03 selesai

Application integration token tenant-scoped kini memiliki penerbitan dengan MFA/re-auth, secret sekali tampil, digest-only storage, expiry 90 hari, allowlist device/target, metadata list, revoke idempoten, dan lookup capability yang menolak token/pemilik/tenant tidak aktif. Admin dapat membaca audit dengan filter, cursor bertanda tangan dan rentang maksimum 31 hari; akses tercatat tanpa memasukkan lokasi atau secret. Perbedaan tipe `audit_events.id` BIGINT terhadap OpenAPI UUID diselesaikan dengan API string desimal untuk menjaga presisi JavaScript dan dicatat sebagai EXEC-004. Cargo fmt/clippy/workspace tests, integration DEVICE-03 pada PostGIS disposable, contract lint/generation, serta 11 tracker tests lulus. Endpoint pemakaian application bearer untuk ingestion/session/realtime tetap task P07/P08. Evidence: [DEVICE-03](./evidence/DEVICE-03.md). Berikutnya dipilih oleh tracker.

## 2026-10-19 — TRACK-01 selesai

Lifecycle `POST/GET /sessions`, detail dan end kini berjalan untuk portal session maupun application bearer. Start memakai Idempotency-Key scoped per tenant/principal/route dengan replay 24 jam, body conflict, dan permission recheck; session scope bearer tetap beririsan dengan allowlist dan hak owner terkini. Lock order organisasi → device → target → session menentukan jam DB setelah lock, sementara constraints existing menegakkan sesi aktif tunggal dan interval non-overlap. End idempoten, tidak memperpanjang ulang timestamp, dan tidak bergantung pada hardware. Migration 011 menambahkan policy function session-scope yang sempit; management/storage docs diselaraskan. Cargo fmt/clippy/workspace tests, contract check, 11 tracker tests, satu server DB integration dan empat storage/migration integrations lulus di PostGIS disposable. AC-12 disimulasikan pada storage boundary dengan transaksi fixture yang memakai lock helper sama; endpoint ingestion tetap task berikutnya dan wajib mematuhi serialisasi ini. Evidence: [TRACK-01](./evidence/TRACK-01.md). Berikutnya dipilih oleh tracker.

## 2026-10-19 — TRACK-02 selesai

HTTP position batch ingestion kini hanya menerima device bearer yang terikat ke device payload, menormalisasi dan memvalidasi titik, memakai fingerprint canonical SHA-256, mengembalikan hasil per-item, serta menerapkan `session_only`/`always`, batas late/future dan retry `TIMESTAMP_NOT_READY`. Dedupe/conflict diperiksa sebelum reevaluasi time/policy; lock order mencegah race dengan session/device lifecycle, dan kegagalan DB me-rollback seluruh batch. Opaque `message_id` dari kontrak kini didukung lewat migration 012 ke TEXT sambil menjaga unique per device. Cargo fmt, clippy, workspace tests, 11 tracker tests, TRACK-01 regression, TRACK-02 integration (mixed batch, schema, policy, late/future, race, wrong-device, rollback) dan storage migration integration lulus pada PostGIS disposable. Pengukuran load dan route end-to-end melalui socket belum dilakukan; heartbeat/realtime tetap TRACK-03/P08. Evidence: [TRACK-02](./evidence/TRACK-02.md). Berikutnya dipilih melalui tracker.

## 2026-10-19 — TRACK-03 selesai

Endpoint device/target latest dan histori device/session selesai. Pembacaan target hanya memakai sesi aktif; histori session memakai interval end-exclusive dan cursor HMAC yang terikat tenant/resource/filter; semua read mengabaikan posisi expired. Authorization `positions:read` kini terpisah dari sesi untuk application token dan tetap dibatasi allowlist serta grant pemilik aktif. `cargo fmt`, clippy/workspace tests, TRACK-03 PostGIS integration, dan storage retention/concurrency integration lulus. Handler integration memanggil Axum langsung; benchmark skala besar dan E2E socket belum dilakukan. Evidence: [TRACK-03](./evidence/TRACK-03.md). Berikutnya LIVE-01.

## 2026-10-19 — LIVE-01 selesai

`POST /v1/ingest/heartbeat` kini menerima device token aktif tanpa body field, menyerialisasi organisasi/device lock, memperbarui `last_contact_at` monotonik, dan tidak membuat posisi/session. Device disable/token revoke ditolak; endpoint membatasi 6/menit dengan burst 3 dan `Retry-After`. Snapshot memisahkan connectivity 180 detik dan freshness posisi 60 detik, keduanya configurable dan tervalidasi saat startup. Cargo fmt/clippy/workspace tests, tes PostGIS LIVE-01 (termasuk no-session, idle/stale-online, threshold, rejected batch, disable/revoke, concurrent contact dan rate limit), serta 11 tracker tests lulus. Limiter masih in-memory per instance; timeout worker/event diteruskan ke LIVE-02. Evidence: [LIVE-01](./evidence/LIVE-01.md). Berikutnya LIVE-02.

## 2026-10-19 — LIVE-02 masih dikerjakan

Route WS ticket browser dan upgrade ditambahkan dengan ticket digest-only, TTL satu menit, consume saat upgrade, Origin check, tenant/session validation berkala, serta validasi permission sebelum subscription acknowledgment. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, dan `cargo test --workspace` lulus (12 passed, 12 ignored; empat migration tests ignored). LIVE-02 belum selesai: application bearer ticket/upgrade, event fan-out, worker timeout, connection/queue bounds dan integration tests masih belum tersedia. Evidence: [LIVE-02](./evidence/LIVE-02.md). Lanjutkan task ini sebelum LIVE-03.

## 2026-10-19 — LIVE-02 selesai

Realtime WebSocket kini memiliki ticket browser/application, direct application bearer handshake, tenant-scoped atomic subscriptions, fan-out posisi/sesi/status, worker freshness/konektivitas, revalidasi otorisasi selama koneksi, batas 5 koneksi/user dan queue 256 event/socket dengan disconnect/resync untuk slow consumer. PostGIS disposable menguji 13 server integration tests dan 4 storage/migration tests; LIVE-02 mencakup Origin, expired login/ticket, replay/restart, cross-tenant/resource denial, fan-out status, bearer dan revocation session/token/grant. Cargo fmt/clippy/workspace tests, 11 tracker tests, dan OpenAPI/generated-client check lulus. Assertion migration count stale diperbarui dari 11 ke 13 agar sesuai semua migrations repo. Event hub process-local sehingga deployment MVP tetap satu instance; load benchmark 100 device belum dijalankan. Evidence: [LIVE-02](./evidence/LIVE-02.md). Berikutnya verifikasi dengan `python3 scripts/project_tracker.py next`.

## 2026-10-19 — LIVE-03 selesai

Snapshot device kini konsisten dengan OpenAPI (`tenant_id`, `observed_at`); event posisi memakai ID decimal string dan UTC RFC3339. `position.updated` hanya memproyeksikan target bila posisi terkait sesi aktif, dan WebSocket mengecek ulang proyeksi terhadap race session end/reassignment. OpenAPI mewajibkan ID pada event posisi dan menyelaraskan field `speed_mps`. LIVE-03 PostGIS test membuktikan posisi/session commit mendahului broadcast, duplicate tanpa event, backfill tak memundurkan latest, snapshot lama kalah dari posisi baru, dan reassignment tidak mengirim proyeksi ke target lama. Suite penuh PostGIS 14 server + 4 storage/migration tests, workspace fmt/clippy/tests, 11 tracker tests dan contract generation lulus. Konsumen tetap harus merge menurut `(recorded_at,id)` serta resync REST setelah reconnect; portal dan load benchmark belum dikerjakan. Evidence: [LIVE-03](./evidence/LIVE-03.md). Berikutnya ambil task dengan `python3 scripts/project_tracker.py next`.

## 2026-10-19 — PORTAL-01 progress

Halaman target/device/credential/tracking-session dibuat pada feature folder dan route Devices; token rotasi meminta reauthentication, hanya tinggal di state lokal, lalu dihapus saat ditutup atau tenant berubah. Form dibuat terpisah, sesi mulai/akhir eksplisit, error/conflict/empty state ditampilkan, viewer tidak mendapat kontrol write. Portal lint, typecheck, 12 UI tests, build, contracts check dan 11 tracker tests lulus. PostGIS disposable integration `device02_credential_lifecycle_permissions_and_device_locking` dijalankan ulang dan lulus; operator tanpa grant dan viewer ditolak server, operator bergrant + MFA diizinkan. Evidence: [PORTAL-01](./evidence/PORTAL-01.md).

PORTAL-01 belum done: Portal spec §2/AC-PORT-04 mensyaratkan kontrol yang mengikuti object grant, tetapi Management API/AuthMe hanya menyediakan role dan tidak mengekspos effective grants pengguna saat ini. Karena role operator bisa memiliki izin per-resource berbeda, UI belum dapat secara akurat menyembunyikan kontrol untuk objek yang tidak diizinkan. Backlog mencatat blocker dan tindakan unblock: tambah route self-effective-grants tenant-scoped, kontrak dan tes isolasi; lalu wire UI sebelum lanjut PORTAL-02.

## 2026-10-19 — PORTAL-01 selesai

PORTAL-01 mencakup daftar/buat/edit target, registrasi/lifecycle device, rotasi token dan start/end tracking session. Portal membaca effective grant anggota sendiri lewat route grants tenant-scoped; admin tetap bisa membaca grant anggota lain, non-admin tidak. UI membatasi tombol per object grant, token tetap transient di state komponen dan dihapus saat close/tenant switch. Kontrak, migration 014, keputusan EXEC-005, serta split AC P09 EXEC-006 diperbarui. Rust fmt/clippy/workspace tests, dua PostGIS authz integration tests, portal lint/typecheck/13 tests/build/contracts checks, 11 tracker tests, validate dan render lulus. Evidence: [PORTAL-01](./evidence/PORTAL-01.md). Live map AC-PORT-01/05/07 dilanjutkan PORTAL-02; history AC-PORT-01/06/08/09 dilanjutkan PORTAL-03 dan belum diklaim selesai. Berikutnya PORTAL-02.

## 2026-10-20 — PORTAL-02 selesai

Route Peta live menggunakan MapLibre dan konfigurasi style URL/MapTiler yang dapat diganti, attribution selalu terlihat, serta tabel status REST tetap bisa dipakai saat tile provider gagal. Snapshot hanya berasal dari sesi aktif target; konektivitas, umur posisi dan tracking session tetap terpisah. Subscription cookie+CSRF+tenant memakai ticket singkat; event dicocokkan menurut tenant/target/device/session dan urutan posisi, heartbeat tidak mengubah marker, reconnect mengambil snapshot ulang, dan tenant switch menutup socket/menghapus cache lama. Target tanpa grant lokasi tidak ditampilkan atau disubscribe. `pnpm lint`, typecheck, 23 portal tests, build, dan 11 tracker tests lulus. E2E memakai fixture API/WS/MapLibre di jsdom, belum backend/provider hidup; vendor MapLibre memberi warning chunk build 1,094.33 kB (296.04 kB gzip) dan dimuat secara lazy. Evidence: [PORTAL-02](./evidence/PORTAL-02.md). Berikutnya PORTAL-03 untuk histori.

## 2026-10-20 — PORTAL-03 selesai

Halaman peta dan histori kini mendukung device maupun session, filter zona lokal → UTC `[from,to)`, cursor table, peta bertahap maksimum 10.000 titik dengan label parsial, histori authorized device nonaktif, dan audit admin organisasi. Query tenant-scoped, respons dicek terhadap organisasi aktif, 403 aman, dan koordinat tidak dipersistenkan. TanStack Table dan Playwright Chromium E2E fixture ditambahkan sesuai frontend stack. Typecheck/lint, 31 portal tests, build, 1 browser E2E Chromium dan 11 tracker tests lulus. Tidak ada backend/provider live pada E2E; MapLibre vendor chunk tetap 1,094.33 kB (lazy loaded). Evidence: [PORTAL-03](./evidence/PORTAL-03.md). Berikutnya OPS-01.

## 2026-10-20 — OPS-01 selesai

Retention worker `retentionctl` menghapus data expired memakai principal capability terpisah, transaksi maksimal 5.000 row, jeda antar-batch, expiry-only RLS/grants, audit 365 hari, retry, outbox <=72 jam, dan metadata auth setelah 7 hari. Alarm dapat dihubungkan dari exit non-zero; worker gagal untuk purge/koneksi maupun retention lag posisi >24 jam. Operasional logs 14 hari dan prosedur tombstone eksternal 45 hari beserta replay restore didokumentasikan. Format, Clippy, workspace tests (13 pass; 14 DB tests ignored secara default), 11 tracker tests, serta seluruh 4 integration storage tests pada Postgres/PostGIS disposable lulus. Worker sukses dan koneksi gagal non-zero diuji. Scheduler, journald host, hard-delete produksi, backup/restore belum dijalankan. Evidence: [OPS-01](./evidence/OPS-01.md). Berikutnya OPS-02.

## 2026-10-20 — OPS-02 selesai

Hardening menambahkan rate limits login/account/IP, management/history/realtime, ingestion device, headers CSP/security dan HSTS bersyarat HTTPS, pool max20/acquire5s/statement10s, serta fixed-label Prometheus metrics. CI kini menjalankan RustSec dan pnpm audit; `tracing-subscriber` diperbarui setelah advisory ditemukan. Portal memakai CSP meta dan `shadcn` ditempatkan sebagai tooling development. Rust format/Clippy/workspace tests (17 lulus; 15 DB tests ignored default), 4 database-backed regression tests PostGIS, portal lint/typecheck/31 tests/build, contract check, 11 tracker tests dan validate lulus. Cargo audit melewati exact pengecualian optional RSA yang tidak aktif pada target Postgres; warning rand non-failing dicatat. Audit dev menemukan advisory braces tanpa patched release dan mengecualikan hanya GHSA tersebut; production audit bersih. `/metrics` perlu dibatasi jaringan monitoring, limiter memory masih single-instance, dan reverse proxy portal wajib menambah `frame-ancestors`/HSTS. Hosted CI/deployment belum dijalankan. Evidence: [OPS-02](./evidence/OPS-02.md). Berikutnya OPS-03.

## 2026-10-20 — OPS-03 backup/restore drill selesai; rollback image dipindah ke P11

Ditambahkan contoh konfigurasi pgBackRest tanpa secret dan runbook backup/PITR, key recovery, tombstone replay, purge, credential invalidation, readiness, serta kriteria rollback kompatibel. Pada PostGIS disposable, pgBackRest full + dua incremental + WAL, enkripsi AES-256-CBC, `check`, PITR terisolasi dan cleanup berhasil. Drill mengukur simulated RPO 1.928 detik dan database recovery-to-pre-open-check RTO 168.526 detik; tenant/admin, tombstone, expiry dan revoked auth diperiksa. Rust format/clippy/workspace tests, satu fresh database migration integration test, tracker tests dan validator lulus. Docker repo masih satu host/failure domain dan API ingress tidak dibuka. Berdasarkan EXEC-007, image sebelumnya tidak dibutuhkan untuk menutup OPS-03 selama runbook menetapkan gate kompatibilitas; uji aktual build terdahulu dipindah menjadi acceptance wajib PILOT-01 sebelum pilot. Baseline planning tetap sama. Evidence: [OPS-03](./evidence/OPS-03.md). Berikutnya OPS-04.

## 2026-10-20 — OPS-04 readiness suite selesai; load benchmark ditunda

Ditambahkan `scripts/ops04_load.py` untuk benchmark sintetis 2 tenant/100 device, seed 1 juta posisi, steady/burst, heartbeat, backfill, page history, EXPLAIN dan proyeksi retensi. Satu seed berhasil menghasilkan 1,000,000 posisi sintetis; relasi positions terukur 515,588,096 byte. Pengulangan cleanup menghabiskan ruang Docker/WAL dan PostgreSQL melaporkan fsync I/O error; container dan volume OPS-04 milik task dihapus. Atas permintaan user, percobaan tmpfs berikutnya dihentikan sebelum hasil load metrics lengkap tersedia. Cargo fmt/clippy/workspace tests lulus, seluruh 15 server + 4 storage DB integration tests lulus, 31 portal tests dan 1 Chromium E2E lulus; lint/typecheck/contracts/build juga lulus. OPS-04 tetap in_progress karena throughput, p95, history page, query plan lengkap dan forecast belum terukur. Lanjutkan sesudah user membersihkan storage; detail bukti/command ada di [OPS-04](./evidence/OPS-04.md).

## 2026-10-20 — OPS-04 selesai

Benchmark sintetis pada PostGIS disposable berhasil untuk 2 tenant/100 device dan 1 juta posisi: 10 posisi/detik selama 120 detik (1,200/1,200; p95 64.50 ms), burst 100/detik selama 60 detik (6,000/6,000; p95 105.78 ms), 100 heartbeat (100/100, tanpa menambah posisi), 10 batch backfill (1,000 posisi), race end-session/ingestion (20/20), dan 30 history page lintas dua tenant (3,000 row; p95 73.68 ms). History awal 50 sampel terkena limiter 30/IP/menit; pengukuran ulang sesuai budget berhasil penuh. Query memakai `positions_latest_device_idx`; estimasi relasi posisi 151.46 GiB/tahun pada 10 posisi/detik kontinu, tidak termasuk WAL/backup/replica/headroom. Cargo, 19 DB integration, 31 portal, satu Chromium E2E dan tracker checks lulus. Container/volume benchmark saja dibersihkan. Evidence: [OPS-04](./evidence/OPS-04.md). Berikutnya PILOT-01.

## 2026-10-20 — RET-01 selesai

Masa simpan posisi sekarang dapat diatur dengan `POSITION_RETENTION_DAYS` (default 365, valid 1–36.500); untuk 60 hari, set nilai 60 pada server dan scheduler retention worker. Ingestion menyimpan expiry per row; worker juga membersihkan histori yang sebelumnya disimpan dengan masa lebih panjang setelah cutoff baru terlewati. RLS worker tetap tidak dapat membaca koordinat. Masa simpan audit/log/token tidak berubah. Migration, `.env.example`, operations/app spec, EXEC-008 dan restore runbook diperbarui. Cargo check/Clippy/workspace tests (18 pass), 4 migration/storage integration tests PostGIS, regression purge cutoff 60 hari dan RLS passed. Evidence: [RET-01](./evidence/RET-01.md). Berikutnya PILOT-01.

## 2026-10-20 — DEVSEED-01 selesai

Ditambahkan bin `devseed` khusus localhost `*_dev`/`*_test`, dengan password via env prompt tersembunyi, hash Argon2id, penerapan migration, transaksi/advisory lock dan no-op idempotent. Seed berisi akun org-admin yang wajib setup MFA, 2 organisasi, 5 target/device/session dan 120 posisi sintetis Jakarta/Surabaya. Verifikasi disposable PostGIS menghasilkan hitungan row sesuai ekspektasi, rerun no-op; test URL safety, Cargo tests dan Clippy lulus. Workspace belum memiliki `.env`/DB development pengguna, jadi database permanen pengguna belum diisi. Panduan browser: [development-environment](./development-environment.md). Evidence: [DEVSEED-01](./evidence/DEVSEED-01.md).


## 2026-10-20 — Environment browser development disiapkan

Atas permintaan user dibuat `.env` mode 0600 dengan password Postgres lokal, key MFA/outbox stabil, password seed, URL loopback dan `POSITION_RETENTION_DAYS=60`. Volume Compose `backend-tracker_postgres_data` belum ada dan dibuat baru. DB/Mailpit sehat; migration dan seed diterapkan di DB lokal persisten. Hitungan diverifikasi: 2 organisasi, 1 akun, 5 target/device/session, 120 posisi. Login API teruji menghasilkan `mfa_enrollment_required`, server `/readyz` siap, portal Vite tersedia di http://localhost:5173/login dan tab browser dibuka. Kode kini mengabaikan URL kosong untuk optional `MAILER_DATABASE_URL`/`PLATFORM_DATABASE_URL`, supaya `.env.example` dapat dimuat tanpa koneksi palsu; 18 unit tests + seed guard lulus. Credentials tetap hanya berada di `.env`; jangan commit file itu.

## 2026-10-20 — Perbaikan CSRF bootstrap untuk browser lokal

Investigasi laporan `CSRF token tidak tersedia` menemukan GET bootstrap mengharuskan header `Origin`, yang dapat tidak dikirim pada GET same-origin browser. Hanya endpoint bootstrap CSRF kini menerima `Referer` dengan scheme+authority exact-match sebagai fallback; mutasi autentikasi tetap mengharuskan `Origin`. Unit test fallback, penolakan port/domain lain, 11 server unit tests, Clippy workspace, dan smoke test GET melalui proxy Vite (HTTP 200, no-store) lulus. Backend dev dijalankan ulang. Evidence tambahan: [AUTH-01](./evidence/AUTH-01.md). Pemilik akun perlu mencoba submit login lagi secara manual.

## 2026-10-20 — Perbaikan 403 login setelah enrollment MFA

Cabang `GET /auth/csrf` saat sesi aktif sekarang menyetel cookie pre-auth pendek dengan token yang sama dengan body dan hash sesi, agar POST login tidak menerima token dari cookie lama setelah sesi MFA sudah terbentuk. Tes integrasi AUTH-01 kini mencakup login ulang setelah refresh CSRF sesi; test compile-only berhasil, tetapi eksekusi DB integration belum tersedia karena `GPS_TEST_DATABASE_URL` disposable belum dikonfigurasi. Format, 11 server unit tests dan Clippy workspace lulus. Backend lokal dimulai ulang. Evidence tambahan: [AUTH-01](./evidence/AUTH-01.md).

## 2026-10-20 — Pemulihan sesi setelah TOTP

Keluhan “Sesi tidak tersedia” menunjukkan route portal `GET /auth/me` belum diimplementasikan. Route sesuai OpenAPI kini mengembalikan profil publik, status MFA dan membership milik user pada sesi tervalidasi; migration menambah fungsi metadata membership yang sempit di bawah role policy owner. Tes integrasi penuh login → TOTP → cookie session → auth/me lulus di PostGIS sementara fresh, migration baru diterapkan ke DB lokal, 11 unit tests/Clippy/format lulus, dan backend development dimulai ulang. AUTH-01 diperbarui dan kembali done dengan evidence lengkap. Evidence: [AUTH-01](./evidence/AUTH-01.md).

## 2026-10-20 — Pemulihan CSRF dari cookie sesi kedaluwarsa

HTTP 401 pada `GET /auth/csrf` terjadi karena browser masih mengirim cookie sesi lama ke halaman login. Bootstrap kini membuang cookie sesi yang tidak lagi valid, tetap menerbitkan pre-auth CSRF, dan mengizinkan login pulih tanpa intervensi manual pada cookie. Tes auth integrasi lulus pada database PostGIS disposable; workspace clippy dan 19 server unit tests lulus; smoke test melalui proxy memastikan 200 serta cookie clear. Backend lokal dimulai ulang. AUTH-01 tetap done dengan regresi tambahan; pemilik akun dapat mencoba submit login lagi. Evidence: [AUTH-01](./evidence/AUTH-01.md).

## 2026-10-20 — Audit 403 seluruh halaman portal

Penyebab umum 403 adalah `session_user` memaksa exact Origin pada handler read-only GET; browser same-origin tidak selalu mengirim Origin. Helper GET kini menerima Referer exact-origin, sedangkan semua mutasi mempertahankan Origin+CSRF. Audit menemukan dan menambahkan route `GET /auth/sessions` yang sebelumnya ada di kontrak/portal tetapi belum ada di server; juga role membership `/auth/me` sekarang mengikuti kontrak `admin`. Cargo server tests (19 lulus), integrasi auth/PostGIS disposable (1 lulus), Clippy, portal typecheck, dan 31 portal tests lulus. Browser: Ringkasan/Organisasi/Devices/Sesi login memuat; Live snapshot REST memuat namun subscription WS ditolak; Histori masih menolak respons tenant yang tidak cocok. Dua masalah terakhir dicatat untuk investigasi terpisah, tidak diatasi dengan melonggarkan tenant checks. Evidence: [AUTH-01](./evidence/AUTH-01.md).

## 2026-10-20 — TENANT-01 batas tenant admin dan realtime selesai

Audit browser menemukan root cause gabungan: server dev memakai `tracker_dev` superuser sehingga RLS terlewati; query list target/device tidak memiliki tenant predicate eksplisit; helper izin admin menerima ID resource tanpa cek kepemilikan tenant. Target foreign lalu diberi snapshot inactive dan ikut subscription, yang ditolak oleh pemeriksaan WebSocket yang lebih ketat. Ditambahkan migration yang memvalidasi identitas resource tenant, helper enumerasi UUID tenant aktif untuk worker realtime, tenant context pada principal session WS, predicate tenant eksplisit, login runtime development auth/app least-privilege serta script provisioning yang idempotent. Integration regression devices lulus termasuk simulated BYPASSRLS; LIVE-02 lulus dengan auth/app capability pools. Browser Histori kini menampilkan 2 device organisasi aktif dan Live menunjukkan “Realtime tersambung”. Format, server tests (19 pass), Clippy, portal typecheck dan 31 portal tests lulus. Evidence: [TENANT-01](./evidence/TENANT-01.md).
