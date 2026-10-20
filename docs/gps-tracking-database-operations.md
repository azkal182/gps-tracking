# Database dan operasional MVP

> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.
> Keputusan desain 19 Oktober 2026. SQL migrations, benchmark dan restore drill belum dijalankan.
> [Technical Decisions](./gps-tracking-technical-decisions.md) · [Management API](./gps-tracking-management-api.md)

## 1. Skema logis dan constraint

PostgreSQL + PostGIS, UUID untuk domain/identity, BIGINT position ID (JSON string), timestamptz UTC, geography(Point,4326). Ekstensi btree_gist untuk exclusion session. Tabel tenant memiliki `tenant_id NOT NULL`, FK organisasi, `UNIQUE(tenant_id,id)` untuk composite references. Device UUID tetap globally unique.

| Tabel               | Kolom/constraint utama                                                                                                                                                                  |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| organizations       | id, name, status active/suspended, timestamps                                                                                                                                           |
| users               | id, email_canonical unique, display_name, password_hash, status pending_activation/active/disabled, platform_admin boolean, auth_version, timestamps                                    |
| memberships         | id, tenant_id, user_id, role, status pending_mfa/active/suspended; unique tenant/user                                                                                                   |
| object_grants       | tenant_id, membership_id, resource_type, target_id/device_id nullable, permission; check tepat satu resource sesuai tipe; composite FK tenant ke membership/target/device; unique grant |
| invitations         | id, tenant_id, email, role, token_hash unique, expires_at, consumed_at/revoked_at; satu pending per tenant/email, resend revokes sebelumnya                                             |
| auth_sessions       | id, user_id, secret_hash unique, csrf_hash, created_at, last_interactive_at, expires_at, revoked_at, reauthenticated_at, auth_version; bukan tenant-scoped                              |
| auth_challenges     | user_id, secret_hash, purpose, expires_at, attempt_count, consumed_at; preauth/enrollment/recovery                                                                                      |
| mfa_factors         | user_id unique, encrypted_secret, nonce, key_version, enabled_at, last_accepted_step                                                                                                    |
| recovery_codes      | user_id, code_hash unique, consumed_at; transaksi sekali pakai                                                                                                                          |
| account_tokens      | user_id, purpose reset/activation/recovery, secret_hash, expires_at, consumed_at/revoked_at                                                                                             |
| targets             | id, tenant_id, name, external_ref nullable, metadata JSON object, timestamps; external_ref unique per tenant jika non-null                                                              |
| devices             | id, tenant_id, protocol, identifier, name, metadata, ingestion_policy, enabled, last_contact_at; unique tenant/protocol/identifier                                                      |
| device_credentials  | id, tenant_id, device_id, token_hash unique, created_at, revoked_at; partial unique active per device                                                                                   |
| application_tokens  | id, tenant_id, owner_user_id, name, token_hash unique, scopes/resource allowlists, expires_at, revoked_at; membership composite ref                                                     |
| tracking_sessions   | id, tenant_id, target_id, device_id, started_at, ended_at; end >= start, composite tenant FKs; active unique per target/device                                                          |
| positions           | id, tenant_id, device_id, message_id, payload_fingerprint, location, measurements, recorded_at, received_at, expires_at, attributes; unique device/message_id; measurement checks       |
| idempotency_records | tenant_id, principal_id, route, key, request_hash, response, expires_at; unique tenant/principal/route/key; TTL 24h                                                                     |
| audit_events        | id, tenant_id nullable untuk platform/auth global, actor_ref, action, object ref, outcome, request_id, occurred_at; append-only application role                                        |
| email_outbox        | id, recipient, template, encrypted_payload, status, attempts, next_attempt_at; tidak menyimpan plaintext reset/invite secret                                                            |

Secret token acak entropy tinggi disimpan SHA-256 digest; password selalu Argon2id. Digest token tidak disamakan dengan password hashing. MFA/email payload perlu reversible encryption karena harus dipakai server; key disimpan di secret mount terpisah dari DB dan backup.

`expires_at` posisi disimpan per baris dalam UTC. Server menghitungnya dari `recorded_at + POSITION_RETENTION_DAYS`; default 365 hari mempertahankan kalender satu tahun (termasuk 29 Februari -> 28 Februari). Ubah nilainya sama pada server dan retention worker melalui `.env`, lalu restart keduanya. Worker memakai nilai itu untuk menghapus data lama, termasuk row yang dibuat saat kebijakan lebih panjang; karena purge dijalankan terjadwal (disarankan per jam), data lama dapat terlihat sampai worker sukses. API hanya membaca `expires_at > now`. Nilai valid 1–36.500 hari. Policy device berubah tidak merelasi ulang target atau menghapus histori lama; perubahan `always` -> `session_only` berlaku pada ingestion baru.

## 2. Isolasi dan authorization

- Repository memerlukan tenant context eksplisit; SQL selalu memfilter tenant. Composite FK mencegah session/position menunjuk objek tenant lain.
- Aktifkan RLS untuk tabel domain tenant, dengan FORCE ROW LEVEL SECURITY, app role non-owner tanpa BYPASSRLS, `SET LOCAL app.tenant_id` dalam setiap transaksi. Missing context fail closed; gunakan `current_setting(..., true)` dan null-safe policy. RLS hanya tenant boundary, object grants tetap service layer.
- Pool connection tidak menyimpan session-level tenant state. Uji reuse connection lintas organisasi. Migration role terpisah. Worker retention/backup dan provisioning platform memiliki role sempit terpisah; role HTTP aplikasi tidak dapat membaca semua tenant.
- Identifikasi token device melalui fungsi lookup SECURITY DEFINER sempit yang menerima digest dan hanya mengembalikan identity/status yang diperlukan; fixed search_path, revoke PUBLIC execute, tidak menerima arbitrary SQL. Setelah lookup, domain access memakai tenant context/RLS biasa. Ini bukan jalur bypass akses lokasi.
- Application bearer juga di-resolve lewat fungsi SECURITY DEFINER sempit berbasis digest yang hanya mengembalikan token/tenant/owner/scopes jika token belum revoked/expired dan owner/tenant aktif. API role tidak mendapat SELECT pada `token_hash`; scope, resource allowlist dan grant owner tetap harus diperiksa setiap operasi melalui policy function.
- Session bearer authorization memakai scope `sessions:read|write` yang terpisah dari scope CRUD target/device, lalu menginterseksikannya dengan allowlist target/device dan permission read/write pemilik yang masih aktif. Fungsi policy tidak memberi akses session hanya karena token terdaftar di tenant.
- users/auth_sessions lintas tenant hanya diakses auth repository; endpoints tidak menerima arbitrary user IDs tanpa pemeriksaan ownership/admin scope. Grants perubahan meningkatkan authz version dan menginvalidasi subscription.

## 3. Transaksi, waktu dan query

- Urutan lock tetap: organisasi shared untuk request tenant (exclusive saat suspend/admin-last-member mutation), lalu device UUID ascending, lalu target UUID ascending, lalu session. Semua jalur create/end/ingestion/credential rotation/disable memakai urutan yang sama.
- Ingestion satu request maksimal 100 item dalam satu transaksi: semantic rejected tidak menggagalkan item lain, error DB menggagalkan seluruh transaksi dan menghasilkan 5xx. Dedupe unique constraint, bukan precheck saja. Canonical hash berdasarkan normalized semantic fields, JSON object key order diabaikan, omitted optional fields dinormalisasi. Duplicate lookup sebelum window/policy ulang; konflik payload rejected per item.
- `received_at` diambil server saat request diterima; start/end session memakai DB clock setelah lock diperoleh, bukan transaction-start timestamp yang bisa usang saat menunggu lock. Clamp end >= start, zero-duration session sah dan interval kosong. Non-overlap device memakai GiST exclusion pada tstzrange `[start,end)`. Future-dated `session_only` tetap TIMESTAMP_NOT_READY sesuai API.
- Device disable/rotation dan ingestion serialize pada device lock. Request yang sudah commit sebelum disable tetap sah; sesudah disable commit semua request baru ditolak. Org suspend analogous pada org lock. Tidak ada otomatis menutup session.
- Latest query menggunakan index `(tenant_id,device_id,recorded_at DESC,id DESC)` dan expiry filter, plus session range untuk target. Tidak memakai cache latest tersendiri pada MVP; evaluasi sebelum/sesudah insert menentukan event setelah commit. Device terbaru dan latest sesi aktif dievaluasi terpisah.
- Event best-effort tidak wajib berurutan. Konsumen mengikat tenant/session dan membandingkan recorded_at/id; status events memuat observed_at agar event lama tidak mengalahkan snapshot terbaru. Worker timeout/freshness tiap 5 detik dan refresh REST menghitung status langsung.
- Tambahan indeks positions expires_at/id untuk purge; sessions tenant/target/start dan tenant/device/start; audit tenant/time/id; memberships user/tenant, grants membership/resource. Tidak membuat spatial index sampai ada query spatial yang membutuhkannya.

## 4. Volume dan retensi

MVP mulai dengan positions unpartitioned untuk menjaga unique device/message_id yang sederhana dan global. Ini bukan jaminan mampu menyimpan 315 juta row pada server kecil. Setelah volume terukur, estimasi storage dan query plan harus memenuhi gate produksi sebelum mengisi histori setahun.

Purge setiap jam, batch 5.000 expired rows per transaksi, pause singkat antarbatch, alert bila oldest expired >24h. Monitor autovacuum/dead tuples, index growth, disk dan WAL. Pada proyeksi >50 juta live rows, atau purge/query melanggar target, lakukan review partitioning sebelum volume tercapai. Time partitioning tidak boleh menghilangkan dedupe: unique constraint pada PostgreSQL partitioned table harus mencakup partition key, sehingga migrasi perlu ledger dedupe terpisah atau desain ekuivalen teruji.

Implementasi worker lokal/deployment: jalankan `cargo run -p server --bin retentionctl` sekali per jam dengan `POSITION_RETENTION_DAYS` yang sama dengan server dan `RETENTION_DATABASE_URL` dari login principal yang hanya menjadi anggota `gps_tracking_retention_worker`. Scheduler harus mengirim non-zero exit/status ke alerting; worker juga gagal bila posisi tertua yang masih expired melewati 24 jam. Setiap transaksi menghapus maksimum 5.000 row, lalu menunggu 100 ms sebelum batch berikutnya. Batch memakai primary key dan predicate expiry tanpa membaca payload; delete PostgreSQL mengunci row saat mutasi, tanpa memberi worker privilege UPDATE hanya demi `SKIP LOCKED`. Sediakan timeout scheduler dan alarm kegagalan proses. Jangan memberi capability ini ke HTTP atau mailer principal.

- Posisi: default 365 hari kalender, dapat dikonfigurasi 1–36.500 hari melalui `POSITION_RETENTION_DAYS`; filter expiry langsung, purge bertahap. Mengubah nilai berlaku pada posisi baru dan membuat worker menghapus posisi lama melewati cutoff konfigurasi.
- Audit management/security/read lokasi: 365 hari; jangan simpan raw coordinates, request bodies atau secret. Audit satu per request/subscription change, bukan satu per heartbeat/event posisi.
- Access/debug logs: 14 hari, redaksi query string, Authorization, cookie, payload auth dan koordinat.
- Session/challenge/token metadata: hapus secret records kedaluwarsa/revoked setelah 7 hari; audit tetap menurut masa simpan audit. Email payload dibersihkan setelah sent, failed/pending max 72 jam dan wajib expiry token dihormati.
- Idempotency: 24h. Dedupe posisi tersedia selama posisi tersimpan; retry sesudah expiry ditolak window waktu. Tidak menjamin message ID selamanya unik setelah deletion, tetapi sender tidak boleh menggunakan ulang ID.
- Device/target hard-delete belum menjadi public API; prosedur operator tervalidasi per tenant/device/range tersedia untuk permintaan penghapusan, audit counts tanpa data, dan deletion tombstone minimal di luar snapshot DB selama 45 hari untuk diterapkan saat restore.

Prosedur penghapusan operator: verifikasi otorisasi dan permintaan di luar sistem, catat case ID; validasi tenant, device, dan rentang UTC serta preview hitungan; jalankan penghapusan dalam transaksi dengan parameter yang tervalidasi; catat jumlah posisi/sesi/grant yang terkena dan hasil tanpa menyalin koordinat; lalu simpan tombstone JSON berisi `case_id`, `tenant_id`, `device_id`, `recorded_from`, `recorded_until`, `deleted_at`, `operator_ref`, dan jumlah per jenis di penyimpanan operasional terenkripsi yang terpisah dari backup database. Tombstone tidak boleh berisi token atau koordinat, wajib ditinjau dua orang, dan dipertahankan 45 hari. Saat restore, sebelum membuka API, replay seluruh tombstone yang masih berlaku secara idempoten, jalankan purge expiry, cabut sesi/token sebagaimana prosedur restore, dan simpan bukti hitungan serta case ID. Jangan menjalankan hard delete produksi sampai prosedur dan otorisasi operator siap.

Log aplikasi dikelola di runtime, bukan tabel database: retensi maksimum 14 hari, rotasi/limit disk wajib. Untuk host systemd gunakan journald dengan `MaxRetentionSec=14day` pada `journald.conf` dan batas disk `SystemMaxUse` yang sesuai; setelah perubahan, restart `systemd-journald` dan verifikasi usia entry tertua secara berkala. Jangan gunakan `docker logs` tanpa driver/rotasi yang punya batas retensi terukur. Format log aplikasi saat ini hanya mencatat bind address dan lifecycle; jangan menambahkan header, query string, body request, cookie, token, atau field koordinat. Scheduler dan supervisor harus mempertahankan status kegagalan retention job sebagai alert; jangan mengandalkan log sukses saja.

## 5. Deployment dan backup

Single-instance Rust/portal + PostgreSQL/PostGIS + Caddy, Docker Compose pada Linux, storage SSD persistent. DB tidak exposed publik; migrasi one-shot sebelum app, readiness sebelum traffic. Container image non-root. Pin image/library versions yang kompatibel saat implementasi; patch upgrades rutin setelah test. TLS wajib di luar loopback.

Rollout memakai expand/contract migrations: backup/preflight, migration kompatibel, deploy app, readiness/smoke test. Bila smoke gagal, rollback image sebelumnya jika schema masih kompatibel; destructive migration tidak dilakukan pada rilis yang sama. Restore bukan rollback rutin dan harus memenuhi RPO/RTO.

Sizing staging awal: app 2 vCPU/4 GiB, DB 4 vCPU/8 GiB; disk ditentukan dari hasil seed/load dan proyeksi setahun, bukan angka kapasitas terjamin. Jika satu host awal, alokasikan total dan headroom OS/backup. Jangan deploy hanya berdasarkan spesifikasi ini tanpa pengukuran storage.

Backup pgBackRest: full mingguan, incremental harian, continuous WAL archive ke storage terpisah terenkripsi; recovery window 30 hari dengan full baseline sebelum window dipertahankan, target hapus backup dependency maksimal 37 hari. Alarm backup/WAL gagal atau WAL lag >5 menit. Key backup terpisah dan prosedur recovery key tercatat.

Target awal RPO <=5 menit, RTO <=4 jam, diuji restore sebelum produksi dan per kuartal. Restore ke lingkungan terisolasi: terapkan deletion tombstones dan purge expiry, invalidasi seluruh sesi/token-link auth serta rotasi credential jika perlu, verifikasi tenant dan last-admin, baru buka API. Backup dapat mengandung lokasi yang sudah terhapus sampai expiry backup; tidak diklaim terhapus serentak dengan live DB.

## 6. Limits dan monitoring

Default awal konfigurabel; HTTP 429 + Retry-After saat limit:

| Jalur              | Batas awal                                                                                              |
| ------------------ | ------------------------------------------------------------------------------------------------------- |
| Login              | 5 gagal/15 menit/account, 30 attempt/15 menit/IP; delay/rate limit, bukan permanent lockout             |
| MFA challenge      | 5 attempts/challenge, TTL 5 menit                                                                       |
| Reset/invite email | 3/jam/recipient, 20/jam/actor atau IP                                                                   |
| Tenant management  | 120 requests/menit/user/tenant, burst 30                                                                |
| Query history      | 30 requests/menit/user/tenant; max 31 hari, page 1000                                                   |
| Position ingestion | 60 requests/menit/device, burst 10; max 100 points dan 256 KiB/request                                  |
| Heartbeat          | 6 requests/menit/device, burst 3                                                                        |
| WebSocket          | 5 koneksi/user, 100 IDs/koneksi; outbound queue 256 events, overflow disconnect dengan instruksi resync |

Rate limits bukan kapasitas server. Login memakai rolling window per IP dan hitungan gagal per akun; limit akun tidak menjadi permanent lockout. Management/history juga membatasi IP dan principal, ditambah kombinasi principal/tenant yang dikirim. Ingestion menggunakan token bucket per device (1 request/detik sustained, burst 10), heartbeat 6/menit burst 3. IP forwarded hanya dipercaya dari peer reverse proxy yang dikonfigurasi. Timeout HTTP 30s, DB statement 10s, pool tunggu 5s dan maksimum 20 koneksi per pool. Limiter aplikasi masih memory per instance; deployment multi-instance perlu shared limiter sebelum diaktifkan.

Server mengekspor Prometheus text format di `/metrics` dengan label route/pool terbatas dan tanpa tenant, device, message, credential, atau koordinat. Endpoint ini tidak diautentikasi; batasi lewat firewall/reverse proxy ke jaringan monitoring tepercaya dan jangan meneruskannya ke publik. Metrics yang aplikasi hasilkan mencakup request/error/duration, auth failure, rate limit, hasil ingestion, WebSocket queue overflow/koneksi aktif, dan koneksi pool. WAL, kapasitas database, dead tuples, backup lag dan antrean SMTP memerlukan exporter/monitor eksternal dan belum diklaim sebagai metrics aplikasi.

API menambahkan CSP ketat untuk respons JSON, nosniff, anti-frame, referrer/permissions policy dan CORP. HSTS hanya diberikan pada HTTPS atau HTTPS yang terkonfirmasi dari proxy tepercaya. Portal memiliki CSP meta allowlist self dan MapTiler; karena meta CSP tidak dapat menetapkan `frame-ancestors` atau HSTS, reverse proxy portal wajib memasang header tersebut pada deployment. CSP portal perlu diperbarui bila origin tile provider diganti. Tidak ada request/body/header sensitif yang dicatat oleh logger aplikasi; reverse proxy wajib meredaksi query string, Authorization, cookie dan payload.

CI menjalankan `cargo audit` untuk graph Rust dan audit pnpm terpisah untuk dependency produksi serta development. `shadcn` adalah CLI development dan berada di `devDependencies`. Per 2026-10-20, audit dev graph menemukan `braces@3.0.3` ([GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/ghsa-vfj7-8cjw-p6xm)) melalui CLI tersebut dan advisory belum menyediakan versi patched; CI mengecualikan hanya GHSA ini sambil tetap menggagalkan advisory high lain. Audit dependency produksi bersih.

RustSec juga mencatat `rsa@0.9.10` (RUSTSEC-2023-0071) di lockfile transitif optional; `cargo tree --target x86_64-unknown-linux-gnu -i rsa --offline` mengonfirmasi tidak ada jalur dependency aktif pada target server Postgres-only, dan advisory belum memiliki upgrade fixed. CI mengecualikan hanya advisory tersebut. `tracing-subscriber` dan `tracing` dinaikkan ke patch yang memperbaiki RUSTSEC-2025-0055. Hapus pengecualian bila fitur/dependency aktif berubah atau upstream menerbitkan perbaikan.

Metrics: request latency/error, ingest stored/rejected/duplicate, heartbeat latency, auth failures, WS queue/reconnect, pool saturation, DB bytes/dead tuples, retention lag, WAL/backup lag, SMTP queue. Hindari metric labels ber-cardinality tenant/device/message IDs.

Target staging: ingestion p95 <=300ms dan page history p95 <=500ms pada fixture yang dinyatakan. Load: 100 device, synthetic 10 points/s steady, 100 points/s burst selama 60s, heartbeat saat idle, backfill batch, 2 tenant, concurrent auth/session-end. Uji seed minimal 1 juta positions untuk correctness/query plan; benchmark volume proyeksi sebelum menjanjikan retensi kapasitas setahun. Angka adalah acceptance target yang harus diukur, bukan hasil benchmark.

## Sumber primer

- [pgBackRest backup/restore](https://pgbackrest.org/user-guide.html)
- [PostgreSQL row security](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [PostgreSQL partitioning](https://www.postgresql.org/docs/current/ddl-partitioning.html)

## 7. Klarifikasi lifecycle kredensial dan field

Token device tidak kedaluwarsa berdasarkan waktu pada MVP; hanya satu aktif, dapat dicabut/dirotasi, dan otomatis dicabut saat device dinonaktifkan. Ini keputusan provisioning perangkat tanpa refresh protocol, bukan jaminan keamanan token yang bocor. Last-contact tidak mengubah umur token. Organization suspend memblokir penggunaannya tanpa menghapus kredensial; reaktivasi hanya memulihkan kredensial yang belum revoked. Audit dan batas rate tetap berlaku.

Nama target/device 1–150 karakter trim, metadata object maksimal 16 KiB, external_ref target nullable 1–128 karakter unik case-sensitive per tenant. Identifier device case-sensitive, 1–128 karakter. Nilai empty external_ref ditolak (gunakan null). Penolakan constraint unik dipetakan menjadi 409, bukan 500.
