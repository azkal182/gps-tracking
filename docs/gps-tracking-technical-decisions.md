# Keputusan teknis MVP

> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.
> Dokumen ini menetapkan pilihan teknis yang sebelumnya terbuka. Kontrak rinci: [Auth & Management API](./gps-tracking-management-api.md). Storage/operasional: [Database & Operations](./gps-tracking-database-operations.md). Urutan pekerjaan: [Implementation Plan](./gps-tracking-implementation-plan.md).

## 1. Ruang keputusan

Cakupan produk tetap: 2 organisasi/sekitar 100 device total untuk awal, multi-organisasi, portal management dan tracking core Rust, HTTP movement-adaptive, session_only default, retensi posisi default 365 hari kalender yang dapat dikonfigurasi, heartbeat 60 detik jika tidak ada ingestion dan timeout 180 detik. Angka sizing sintetis bukan prediksi traffic riil.

Keputusan di dokumen ini menggantikan label usulan/terbuka pada revisi terdahulu. Dependency versions dipilih kompatibel saat scaffold dan dikunci; bukan keharusan memakai major terbaru tanpa pengujian. Kebutuhan akun provider, domain, secret dan kapasitas disk aktual merupakan input deployment, bukan keputusan desain yang harus menghambat implementasi lokal.

## 2. Frontend, repository dan delivery

- React + Vite + TypeScript strict, Tailwind CSS, shadcn/ui.
- React Router (Declarative Mode), TanStack Query, React Hook Form + Zod + resolvers, openapi-typescript + openapi-fetch, TanStack Table untuk tabel management.
- Zustand hanya ditambahkan untuk kebutuhan UI state lintas komponen yang terbukti; tidak menyalin server data/cache lokasi.
- Vitest + React Testing Library untuk perilaku komponen; Playwright untuk flow utama dan authorization end-to-end. ESLint dan Prettier untuk portal. pnpm sebagai package manager, lockfile committed.
- `apps/server` (Rust), `apps/portal` (frontend), `crates`, `migrations`, `contracts`, `deploy`, `docs`. Cargo workspace dan pnpm bekerja dalam satu repository tanpa Nx/Turborepo pada MVP.
- Multi-stage Docker build: build frontend dan Rust; runtime membawa binary dan static assets tanpa Node server. Satu origin, Caddy menangani TLS/reverse proxy. `/v1` selalu API dan tidak boleh fallback ke SPA. Vite dev proxy mendukung API dan WS.
- Generated OpenAPI client diperiksa pada CI terhadap perubahan kontrak; form schema tetap memiliki tujuan UI dan tidak menggantikan validasi backend.

## 3. Akun, password dan MFA

- Login email/password internal, tanpa self-registration. Email disimpan dalam bentuk canonical case-insensitive, unik; jangan melakukan normalisasi provider-specific seperti menghapus titik Gmail.
- Password 15–128 karakter, mendukung spasi/Unicode, tanpa aturan wajib kombinasi karakter atau rotasi berkala. Tolak password umum/terkompromi menggunakan daftar lokal terkurasi; tidak mengirim password ke pihak ketiga.
- Argon2id dengan salt unik dan encoded parameters; awal `m=65536 KiB`, `t=3`, `p=1`, benchmark saat deployment. Batasi concurrency hashing; konfigurasi tidak boleh di bawah minimum OWASP (19 MiB, t=2, p=1).
- MFA awal TOTP, 6 digit, step 30 detik, toleransi satu step di kedua sisi; verifier memakai library teruji. Secret terenkripsi AEAD dengan key di luar DB, nonce unik, versioned key ID. Cegah penggunaan ulang step yang sudah diterima secara atomik.
- MFA wajib bagi platform admin, org admin, dan operator dengan izin credentials; untuk akun lain opsional. Bila sudah diaktifkan, MFA tetap diwajibkan pada login walau role diturunkan.
- Login tanpa MFA yang diperlukan menghasilkan challenge terbatas 5 menit, maksimal 5 percobaan; bukan sesi aplikasi penuh. Enrollment awal mandatory MFA hanya memberi akses enrollment/logout, tidak management/location.
- Recovery codes: 10 kode acak, masing-masing minimal 128 bit entropy, hash at rest, sekali pakai atomik. Recovery memerlukan password + satu kode; regenerasi membutuhkan recent authentication dan membatalkan seluruh kode lama. Sesi recovery hanya boleh re-enroll MFA/logout; enrollment pengganti menaruh secret di pending factor tanpa menimpa faktor aktif sebelum konfirmasi; setelah enrollment sukses cabut semua sesi dan minta login ulang.
- Reset password tidak menghapus MFA. Kehilangan TOTP dan semua recovery codes: tidak ada self-service bypass. Operator melakukan identity verification di luar portal, mencatat case/reference dan menggunakan CLI server untuk membuat recovery authorization single-use 15 menit. Pengguna tetap membuktikan password dan email, lalu wajib enroll ulang; notifikasi dan audit, semua sesi lama dicabut. Platform admin terakhir menggunakan prosedur server yang sama melalui akses infrastruktur; possession email saja tidak cukup.
- WebAuthn/passkeys menjadi peningkatan P1. TOTP dipilih untuk implementasi awal yang familiar dan tidak diklaim phishing-resistant.

## 4. Sesi browser dan tenant

- Opaque session secret acak 256 bit; hash disimpan di PostgreSQL. Cookie `__Host-tracking_session`, `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, tanpa Domain. Tidak menyimpan auth token dalam localStorage.
- Idle timeout 30 menit, absolute timeout 12 jam; tanpa remember-me pada MVP. Aktivitas REST interaktif memperbarui idle; ping/WS/background polling tidak menjaga login selamanya. Idle touch maksimal sekali per menit.
- CSRF token synchronizer terikat sesi, dikirim lewat `X-CSRF-Token` untuk semua mutation berbasis cookie, termasuk logout. Verifikasi Origin pada mutation dan WS; JSON-only, CORS same-origin. Login memakai pre-auth CSRF cookie/token dan rotasi setelah login. CSRF bukan pengganti authZ.
- Recent authentication maksimal 5 menit untuk rotasi secret, perubahan password/MFA, grants/role, membership, organisasi dan penerbitan application token: password + MFA jika enrolled/mandatory. Tidak mengharuskan re-auth untuk setiap posisi/query atau start/end session.
- Portal mengirim `X-Organization-Id` pada request tenant-scoped. Backend selalu memverifikasi membership aktif; header bukan bukti izin. Cookie mengidentifikasi user, tidak menyimpan satu tenant global yang diam-diam berubah pada tab lain.
- Satu tab bekerja pada satu organisasi. Switch membatalkan request, menghapus cache, menutup WS lama dan memakai tenant baru; tab lain boleh tetap di organisasi sebelumnya. Setiap respons/cache/subscription terikat tenant eksplisit sehingga tidak terjadi salah konteks lintas tab.
- Logout, password reset, recovery dan penonaktifan user mencabut sesi. Membership/grant diperiksa setiap request dan invalidation menutup WS terkait. Worker setiap <=5 detik memeriksa expiry/revocation WS; pengiriman event juga memeriksa izin terbaru. Token/ticket tidak boleh mempertahankan izin lama.
- Application bearer tokens tetap tersedia untuk integrasi server eksternal, terikat satu tenant, owner membership, scopes dan grant; TTL 90 hari, tanpa refresh otomatis, hash at rest, revoked bila owner kehilangan akses. Hanya admin dengan recent MFA dapat menerbitkan; token tidak dapat mengelola akun, role, membership, platform atau menerbitkan credential lain. Device bearer token hanya ingestion/heartbeat.

## 5. Onboarding dan akses

- Platform admin dibuat dengan CLI bootstrap one-time melalui console server, tidak ada public bootstrap endpoint/password default. CLI menyiapkan aktivasi email; MFA wajib sebelum management.
- Platform admin membuat organisasi dan undangan admin pertama; email melalui SMTP TLS yang dikonfigurasi. Gunakan Mailpit lokal; production memakai relay SMTP terautentikasi milik operator. Modul SMTP adapter tidak bergantung vendor.
- Invite TTL 72 jam, reset password 30 menit, single-use/revocable. Link memakai fragment untuk membawa secret ke form kemudian POST body, halaman tanpa third-party scripts, `Referrer-Policy: no-referrer`; GET tidak mengonsumsi token agar mail scanner tidak mengaktifkan akun.
- Invite akun yang sudah ada memerlukan login ke email yang cocok; password tidak pernah ditimpa invite. Akun baru menetapkan password; acceptance membuktikan email. Required MFA enrollment sebelum akses privileged. Membership belum aktif sampai accepted.
- Admin terakhir tidak dapat dinonaktifkan/dihapus/diturunkan; transaksi mengunci organisasi untuk mencegah race. Platform tidak otomatis membaca lokasi.
- Operator perlu grant organisasi `targets:create`/`devices:create`; pencipta mendapat grant read/write objek baru. Credential write tetap grant terpisah. Viewer hanya read. Session read mengikuti akses target; session write memerlukan write target dan device. Histori device perlu device read eksplisit.
- Suspend organisasi memblokir semua tenant API/ingestion/WS tetapi tidak menghapus histori/menutup session; timestamp sesi tetap benar dan late-window masih berlaku setelah reaktivasi. Reaktivasi tidak menghidupkan token yang sudah revoked. Suspend device mencabut credential aktif; aktivasi ulang memerlukan token baru. Ingestion berjalan dan perubahan status diserialisasi pada lock yang sama.

## 6. Peta dan histori

- Renderer MapLibre GL JS; provider awal MapTiler Cloud, style streets, attribution selalu ditampilkan. Key browser dibatasi origin dan hanya map-read; ini public configuration, bukan secret backend. Tidak ada pembelian/aktivasi layanan otomatis dalam keputusan ini.
- Peta meminta tile/style kepada provider, sehingga provider dapat melihat IP dan area tile yang diminta. Koordinat tracking, token dan histori tidak dikirim sebagai API payload provider; overlay dirender lokal. Terapkan CSP allowlist minimum dan jangan pakai analytics pihak ketiga pada halaman sensitif.
- Operator menyediakan akun/key serta paket MapTiler yang sesuai izin penggunaan dan volume saat deployment. Tidak menjanjikan layanan gratis. Tile failure tidak memblokir tabel/status/histori; style URL konfigurabel untuk penggantian provider/self-hosting nanti.
- Histori default 24 jam, maksimum rentang 31 hari per query, dapat memilih periode mana pun dalam retensi setahun. `limit` default 100, maksimum 1000; cursor mengikat tenant/resource/filter/sort dan tidak menjanjikan snapshot penuh.
- Peta memuat paling banyak 10.000 titik per tampilan secara bertahap; saat tercapai tampilkan “sebagian data” dan minta rentang dipersempit. Tabel tetap bisa menelusuri halaman berikutnya; tidak diam-diam menghapus sampel. Render garis sampel, tidak map matching. Waktu ditampilkan dengan zona eksplisit (default zona browser); API UTC.

## 7. Batas klaim

Keputusan desain selesai untuk memulai implementasi. Migration SQL, OpenAPI machine-readable, code, E2E dan load test adalah deliverable implementasi, bukan dianggap sudah tersedia. Kapasitas, SLO dan kesiapan production hanya boleh diklaim setelah pengukuran/restore drill. Domain, SMTP credentials, tile key dan infrastruktur perlu disediakan ketika deployment, tanpa menunda pekerjaan lokal.

## Rujukan

- [OWASP password storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [OWASP session management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [OWASP MFA](https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html)
- [OWASP password reset](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
- [MapTiler terms](https://www.maptiler.com/terms/cloud/), [pricing](https://www.maptiler.com/cloud/pricing/)

Timeout, TTL dan angka limit pada dokumen ini adalah keputusan proyek yang dapat dikonfigurasi, bukan angka yang diklaim diwajibkan oleh sumber di atas.
