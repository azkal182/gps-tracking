# OPS-02 — Security limits dan observability

Tanggal: 2026-10-20

## Implementasi

- Middleware mengklasifikasikan request ke label rute tetap; memasang rate limit login per IP, 5 kegagalan per akun/15 menit, auth 60/menit, management 120/menit per IP/principal/tenant, history 30/menit per IP/principal/tenant, dan realtime 120/menit. `Retry-After` disertakan pada respons 429. IP forwarded hanya dipercaya dari alamat proxy yang dikonfigurasi.
- Position ingestion dibatasi 1 request/detik/device dengan burst 10. Heartbeat tetap 6/menit dengan burst 3; WebSocket tetap dibatasi 5 koneksi/user, 100 subscription IDs/koneksi, queue 256 dan timeout send.
- Server memasang CSP API, nosniff, anti-frame, referrer/permissions policy dan CORP. HSTS hanya diberikan untuk HTTPS langsung atau HTTPS yang diverifikasi lewat proxy tepercaya. Portal memiliki CSP meta self + MapTiler dan tidak memuat analytics pihak ketiga.
- Pool runtime auth/tracking/platform maksimum 20 koneksi, wait maksimum 5 detik dan PostgreSQL statement timeout 10 detik. Metrics Prometheus `/metrics` memakai label route/pool tetap tanpa tenant/device/message/credential/coordinate labels.
- Audit login gagal disimpan sebagai digest email, bukan alamat email; code paths logging tidak menulis request body, token, cookie atau koordinat. `shadcn` dipindah ke `devDependencies`.
- CI menjalankan `cargo audit` serta audit pnpm production dan development. `tracing-subscriber`/`tracing` dinaikkan untuk memperbaiki RustSec RUSTSEC-2025-0055.

## Evidence dan verifikasi

- `cargo fmt --all -- --check` — lulus.
- `cargo clippy --workspace --all-targets -- -D warnings` — lulus.
- `cargo test --workspace` — 17 lulus, 15 ignored karena butuh database; 4 migration tests storage juga ignored.
- Disposable PostGIS: `ops02_runtime_pool_sets_statement_timeout`, `auth_login_csrf_timeout_logout_and_revocation_lifecycle`, `mfa_login_recovery_and_reenrollment_are_atomic_and_restricted`, dan `live02_websocket_ticket_subscription_and_revoke` — semuanya lulus. Ini memverifikasi statement timeout, CSRF/origin/session, MFA/recovery policy dan WS replay/revocation.
- `cargo audit --ignore RUSTSEC-2023-0071` — tidak ada vulnerabilities yang menggagalkan audit. `rsa@0.9.10` diabaikan karena tidak punya jalur dependency aktif pada target Postgres-only (`cargo tree --target x86_64-unknown-linux-gnu -i rsa --offline` memberi “nothing to print”) dan belum ada fixed release. Audit memberi warning RUSTSEC-2026-0097 untuk `rand` karena skenario custom logger; aplikasi tidak memasang custom logger.
- `pnpm audit --prod --audit-level high` — bersih. Audit development menemukan satu advisory high `braces@3.0.3` melalui CLI `shadcn`; belum ada patched version upstream. CI mengecualikan hanya GHSA-vfj7-8cjw-p6xm dan tetap menolak advisory high lain. Rincian ada di database operations.
- `pnpm install --frozen-lockfile`, `pnpm lint`, `pnpm typecheck`, `pnpm test` (31 lulus), `pnpm build`, `pnpm contracts:check` — lulus. Build tetap memberi warning ukuran vendor MapLibre (1,094.33 kB / 296.04 kB gzip), sudah ada sebelum task ini.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — 11 lulus. `python3 scripts/project_tracker.py validate` — valid, 40 task.
- Container PostGIS disposable `gps-ops02-postgres` sudah dihentikan dan dihapus. Tidak ada hosted GitHub Actions atau production deployment yang dijalankan.

## Batas operasional dan tindak lanjut

Rate limit tetap memory per instance; deployment multi-instance memerlukan shared limiter. Batasi `/metrics` melalui firewall/reverse proxy ke jaringan monitoring tepercaya. Reverse proxy portal harus menambahkan `frame-ancestors` dan HSTS pada response headers; CSP meta tidak dapat memasang header tersebut. Hapus pengecualian advisory setelah upstream dependency memperbaiki `braces`, dan evaluasi ulang ignored RSA jika fitur SQLx selain Postgres diaktifkan. Berikutnya OPS-03 untuk backup/restore dan rollback drill.
