# Execution decision log

Baseline final v1.0 tetap pada docs planning. Tidak ada perubahan fitur produk pada pembuatan workflow ini.

## 2026-10-19 — EXEC-001

- Keputusan: skills repository-local, canonical JSON backlog dan generated progress views; P00 terpisah dari implementasi MVP.
- Alasan: agent baru bisa melanjutkan dari bukti aktual tanpa menyamakan planning FINAL dengan feature DONE.
- Dampak: AGENTS.md, .agents/skills, scripts/project_tracker.py, docs/execution.
- Tidak mengubah scope, auth, tenant, retensi, heartbeat atau stack.

Untuk keputusan berikut, catat ID/tanggal, pemicu, keputusan, opsi yang ditolak bila relevan, requirement/kontrak yang terdampak, migration/test impact dan tautan task. Jangan menambah persetujuan user yang tidak pernah diberikan.

## 2026-10-19 — EXEC-002 — Principal untuk WebSocket ticket

- Pemicu: API Contract §8.1 memberi contoh `POST /realtime/tickets` dengan application bearer untuk integrasi non-browser; Management API §5 secara eksplisit mewajibkan browser memakai cookie + CSRF + tenant header dan menyatakan integrasi non-browser dapat memakai bearer saat handshake.
- Keputusan: `POST /realtime/tickets` menerima portal session + CSRF atau application bearer dengan `tracking:subscribe`. Browser wajib mengirim `X-Organization-Id`; application token sudah terikat tenant. Device bearer tetap dilarang. Direct application-bearer handshake ke `/ws/tracking` juga tetap didukung.
- Alasan: mempertahankan contoh integrasi pada API Contract tanpa memberi token hak management; kedua mode tetap terikat tenant, scope dan object grants terkini.
- Dampak: kontrak OpenAPI `POST /realtime/tickets` dan `GET /ws/tracking`; tidak mengubah data model atau migration. Authorization tetap harus diperiksa saat subscribe dan delivery; ticket single-use, maksimal 60 detik.
- Validasi lanjutan: CONTRACT-02 memvalidasi mode auth dan schema; runtime auth/revocation tests tetap menjadi task implementasi auth/realtime.

## 2026-10-19 — EXEC-003 — Aktivasi awal, recovery operator dan mailer capability

- Pemicu: AUTH-03 mensyaratkan CLI bootstrap yang mengirim aktivasi dan recovery operator dengan bukti email, tetapi baseline hanya menyebut alur dan schema tidak menyediakan status pending activation, endpoint penukaran aktivasi, tujuan email proof terpisah, atau hak DB untuk mailer.
- Keputusan: tambahkan status `pending_activation` dan `POST /auth/activate` yang menetapkan password pertama setelah token activation sekali pakai; akun bootstrap tidak memperoleh password yang dapat digunakan login. Operator recovery memakai authorization token 15 menit plus email proof token 15 menit dengan purpose terpisah dan hanya menghasilkan challenge enrollment MFA 5 menit. Worker SMTP memakai capability DB terpisah yang hanya dapat mengakses outbox. Outbox plain SMTP hanya diizinkan ke loopback/Mailpit; relay non-local wajib TLS.
- Alasan: tidak menyamakan activation dengan login, memisahkan dua bukti recovery, dan menjaga mailer di luar akses lokasi maupun API request path.
- Dampak: migration `202610190007_auth_email_worker.sql`, endpoint/schema OpenAPI activation, `authctl`, konfigurasi SMTP dan test AUTH-03. Tidak mengubah autentikasi tenant atau hak akses lokasi.
- Validasi lanjutan: AUTH-03 menjalankan negative/replay/expiry dan Mailpit integration tests; default deployment SMTP/TLS credentials tetap input operator pada tahap deployment.

## 2026-10-19 — EXEC-004 — Representasi ID audit di API

- Pemicu: OpenAPI `AuditEvent.id` memakai schema UUID, tetapi migration `202610190003_tracking_domain.sql` menetapkan `audit_events.id BIGINT GENERATED ALWAYS AS IDENTITY`; endpoint audit dan keyset cursor tidak dapat memenuhi kedua bentuk sekaligus.
- Keputusan: simpan BIGINT untuk indeks audit append-only dan expose sebagai string desimal positif pada Management API. JSON number dihindari agar klien JavaScript tidak kehilangan presisi. Cursor audit membawa ID desimal dan timestamp di payload HMAC terikat tenant/filter; cursor resource UUID lain tetap memakai validator UUID yang ada.
- Dampak: schema `AuditEvent.id` dan contoh OpenAPI diperbaiki; generated TypeScript tetap bertipe `string`. Tidak ada migration/schema DB baru.
- Task: DEVICE-03; validasi `pnpm contracts:check` dan test audit pagination di database disposable.

## 2026-10-19 — EXEC-005 — Anggota membaca grant miliknya sendiri

- Pemicu: PORTAL-01 memerlukan tombol per-object mengikuti izin aktual, tetapi `AuthMeResponse` hanya membawa role membership. Operator dapat memiliki grant yang berbeda antar target/device. Endpoint GET daftar grant semula hanya mengizinkan org admin sehingga UI tidak dapat menampilkan permission efektif.
- Keputusan: anggota aktif boleh membaca daftar grant miliknya sendiri di tenant aktif melalui endpoint grants yang sudah ada; org admin tetap boleh membaca grant semua anggota. PUT tetap admin-only + recent auth. Database function mengecek sesi aktif, membership tenant, target membership, dan self-or-admin agar role DB tidak memperluas akses.
- Alasan: perubahan terkecil yang memberi portal sumber permission otoritatif tanpa menambahkan endpoint baru atau membuka grant anggota lain. API tetap tenant-scoped; grant yang dikembalikan hanya metadata akses, bukan lokasi/secret.
- Dampak: Management API/OpenAPI GET grants, database migration `202610190014_member_self_grant_read.sql`, handler tenant membership check, portal query dan kontrol per-object. Generated client diregenerasi dari OpenAPI.
- Validasi: kontrak lint/generation, integration negative test self-vs-other/cross-tenant, role denial dan portal operator-grant UI test.
- Task: PORTAL-01.

## 2026-10-19 — EXEC-006 — Acceptance P09 dibagi per slice portal

- Pemicu: backlog PORTAL-01 mencantumkan AC-PORT-01 yang mencakup provisioning, posisi, peta dan histori, sementara PORTAL-02/03 secara eksplisit membangun peta dan histori. PORTAL-09 juga mencakup penonaktifan serta pembacaan histori.
- Keputusan: verifikasi AC-PORT-01/09 dibagi menurut deliverable: PORTAL-01 untuk provisioning/session/lifecycle dan izin; PORTAL-02 untuk live map journey; PORTAL-03 untuk history journey dan data retained. Acceptance skala P09 tetap mengharuskan seluruh alur end-to-end setelah PORTAL-03.
- Alasan: setiap task dapat selesai pada lingkupnya tanpa mengklaim fitur peta/histori sebelum ada implementasi.
- Dampak: hanya acceptance dan evidence execution backlog; baseline requirement tidak berubah.
- Task: PORTAL-01, PORTAL-02, PORTAL-03.

## 2026-10-20 — EXEC-007 — Waktu bukti kompatibilitas rollback

- Pemicu: OPS-03 berada di P10 sebelum image aplikasi dibuat, tetapi acceptance
  task meminta uji kompatibilitas image release terdahulu. Repository belum
  memiliki image tersebut. Baseline database operations §5 mensyaratkan rollback
  hanya jika schema kompatibel; implementation plan §11 menempatkan migrasi dan
  rollback operasional pada rollout pilot.
- Keputusan: OPS-03 dapat selesai dengan backup/PITR dan RPO/RTO yang terukur,
  tombstone/expiry/auth cleanup yang diuji, serta runbook yang menetapkan syarat
  rollback. Uji aktual build/image terdahulu terhadap schema setelah migration
  menjadi gate wajib PILOT-01. Build terdahulu tidak harus pernah production;
  image kandidat berurutan dapat diuji di staging disposable.
- Alasan: jangan menahan verifikasi backup/restore pada artefak yang belum ada di
  fase development, sekaligus jangan melemahkan syarat kompatibilitas sebelum
  pilot. Jika uji gagal atau tidak tersedia, pilot tetap tertahan dan tim
  melakukan forward-fix atau memilih recovery terotorisasi.
- Dampak: acceptance OPS-03 dan PILOT-01 serta evidence/handoff execution. Tidak
  mengubah baseline produk, kontrak API/DB, atau memberi otorisasi deployment.
- Validasi: backup/PITR disposable dan hasil RPO/RTO tercatat di evidence OPS-03;
  pemeriksaan binary rollback aktual menjadi acceptance PILOT-01.


## 2026-10-20 — EXEC-008 — Retensi posisi configurable

- Pemicu: pengguna meminta konfigurasi masa simpan posisi dari `.env`, termasuk nilai 60 hari.
- Keputusan: pertahankan default 365 hari, tambahkan `POSITION_RETENTION_DAYS` (1–36.500) yang dibaca server dan `retentionctl`. Expiry disimpan per posisi; worker menggunakan cutoff konfigurasi untuk purge row baru maupun histori lama saat kebijakan diturunkan.
- Alasan: generated column yang selalu satu tahun tidak dapat mengikuti konfigurasi deployment. Expiry per-row menjaga filter baca tetap langsung dan memungkinkan rollout tanpa session-level database state.
- Dampak: butuh migration yang mengubah generated expiry menjadi nilai tersimpan dan trigger default 365 untuk penulis/fixture lama. Operasional wajib mengatur nilai identik di server dan scheduler worker. Baseline MVP 365 hari tetap menjadi default; angka khusus deployment adalah override.
- Validasi: migrasi fresh/upgrade dan tes expiry 60 hari serta default kalender harus lulus.
