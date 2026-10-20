# GPS Tracking — Portal Management MVP

> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.
> **Tanggal:** 19 Oktober 2026  
> **Dokumen terkait:** [App Spec](./gps-tracking-app-spec.md), [Backend Summary](./gps-tracking-backend-summary.md), [API Contract](./gps-tracking-api-contract.md)

## 1. Tujuan dan batas produk

Portal web menjadi bagian MVP dalam proyek yang sama dengan backend. Tim menggunakan portal untuk management organisasi dan akses, registrasi/provisioning device, pengelolaan target serta tracking session, peta monitoring, dan histori lokasi. Backend tetap memvalidasi setiap akses dan menjadi sumber kebenaran data; menyembunyikan tombol bukan pengganti authorization.

Keputusan yang sudah disepakati: login internal, multi-organisasi, target/device/session tetap tersedia, HTTP ingestion, default `session_only`, retensi posisi satu tahun kalender. Peran awal admin/operator/viewer beserta izin khusus penerbitan token mengikuti diskusi yang disetujui untuk dimasukkan ke plan.

Frontend dipilih: React + Vite, Tailwind CSS dan shadcn/ui. TypeScript dan library inti ditetapkan dalam [Frontend Stack](./gps-tracking-frontend-stack.md); peta memakai MapLibre + MapTiler Cloud. Alur organisasi/undangan disepakati pada §3.1. Tidak ada kebutuhan membangun aplikasi pengirim posisi dalam dokumen ini. Billing, export massal, analitik perjalanan, geocoding dan route optimization berada di luar MVP.

## 2. Halaman dan kebutuhan P0

| ID           | Halaman/kemampuan   | Perilaku wajib                                                                                                         |
| ------------ | ------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `FR-PORT-01` | Akun dan organisasi | Login/logout, perubahan password, recovery, daftar/pencabutan sesi login; konteks organisasi aktif jelas               |
| `FR-PORT-02` | Anggota dan akses   | Admin mengelola membership, peran dan grant objek; daftar hanya dalam organisasi yang diizinkan                        |
| `FR-PORT-03` | Devices             | Registrasi, daftar/detail, status aktif, penerbitan/rotasi token dan penonaktifan                                      |
| `FR-PORT-04` | Targets             | Daftar/detail, membuat dan memperbarui target yang diizinkan                                                           |
| `FR-PORT-05` | Tracking sessions   | Pilih target/device satu organisasi, mulai/akhiri sesi, lihat sesi aktif serta histori; tampilkan konflik secara jelas |
| `FR-PORT-06` | Peta monitoring     | Lokasi terakhir, waktu rekam/penerimaan, status freshness dan sesi aktif; hanya objek yang diizinkan                   |
| `FR-PORT-07` | Histori lokasi      | Pilih device atau session, filter waktu, titik/jalur di peta dan daftar posisi dengan pagination                       |
| `FR-PORT-08` | Audit               | Aktivitas management dan akses sensitif sesuai kewenangan tanpa secret atau koordinat mentah di log                    |

Peta memperlihatkan titik tersimpan dan garis penghubung sampel, bukan klaim rute jalan sebenarnya. Posisi offline diberi label lokasi terakhir beserta waktunya. Keadaan kosong, tidak ada sesi, belum ada posisi, koneksi putus, loading, akses ditolak dan kegagalan request memiliki tampilan eksplisit.

## 3. Peran dan batas izin

| Aksi dalam organisasi         | Admin                    | Operator                                           | Viewer               |
| ----------------------------- | ------------------------ | -------------------------------------------------- | -------------------- |
| Kelola membership/peran/grant | Ya                       | Tidak                                              | Tidak                |
| Registrasi device/target      | Ya                       | Dengan izin create organisasi                      | Tidak                |
| Perbarui target/device        | Seluruh objek organisasi | Objek yang diberikan                               | Tidak                |
| Terbitkan/rotasi token device | Ya                       | Objek yang diberikan + `devices:credentials:write` | Tidak                |
| Nonaktifkan device            | Ya                       | Objek yang diberikan + izin write                  | Tidak                |
| Mulai/akhiri tracking session | Ya                       | Memerlukan akses write terkait target dan device   | Tidak                |
| Peta/histori                  | Seluruh objek organisasi | Objek yang diberikan                               | Objek yang diberikan |
| Ubah policy menjadi `always`  | Ya, diaudit              | Tidak                                              | Tidak                |
| Lihat audit organisasi        | Ya                       | Tidak secara default                               | Tidak secara default |

Detail scope create dan grant otomatis untuk pencipta objek harus ditentukan dalam kontrak authorization. Role operator saja tidak memberi akses ke seluruh organisasi. Grant target/session tidak memberi akses seluruh histori device. Viewer tidak pernah menerima token device.

Pengelola platform merupakan peran operasional terpisah, bukan admin semua organisasi. Pengelola membuat organisasi dan menyiapkan admin awal tanpa akses otomatis ke koordinat. Akses bantuan lintas organisasi tidak tersedia MVP. Suspend/reaktivasi organisasi mengikuti [Management API](./gps-tracking-management-api.md).

### 3.1 Organisasi dan akun — disepakati

1. Pengelola platform membuat organisasi dan menyiapkan admin pertamanya melalui alur aktivasi akun; tidak menerima atau menentukan password pengguna untuk pemakaian bersama.
2. Admin organisasi mengundang anggota dan menetapkan peran/grant dalam organisasinya sendiri.
3. Tidak ada registrasi publik atau pembuatan organisasi mandiri pada MVP.
4. Satu akun dapat memiliki membership di beberapa organisasi. Pemilihan organisasi aktif memverifikasi membership dan tidak menggabungkan izin antarorganisasi.
5. Pengelola platform tidak otomatis memperoleh akses lokasi/histori organisasi. Peran platform tidak boleh menjadi bypass authorization objek.

Undangan TTL 72 jam via SMTP, bootstrap CLI, dan proteksi admin terakhir ditetapkan pada Management API. Aturan: undangan terikat organisasi/email/peran, diterima setelah pembuktian kepemilikan alamat, dan tidak memberikan akses sebelum diterima. Ini tidak membuka endpoint registrasi publik.

Acceptance tambahan: registrasi publik/create organisasi oleh pengguna biasa ditolak; admin A tidak dapat mengundang anggota ke B; pengguna dengan dua membership hanya melihat data tenant aktif; pengelola platform tanpa grant lokasi tidak dapat membaca koordinat.

### 3.2 Login, MFA dan pemulihan — disepakati

- Login menggunakan email dan password akun internal.
- Undangan/aktivasi akun dan reset password menggunakan tautan email sekali pakai, dengan masa berlaku dan kemampuan pencabutan. TTL serta provider pengiriman ditentukan pada desain kontrak.
- MFA wajib untuk pengelola platform, admin organisasi, serta operator yang memiliki izin penerbitan/rotasi token device. MFA opsional bagi pengguna lain.
- Kewajiban MFA mengikuti gabungan hak istimewa akun di seluruh membership, bukan hanya organisasi aktif. Perubahan peran/grant yang membuat MFA wajib harus memblokir penggunaan hak istimewa sampai enrollment dan verifikasi MFA selesai; tidak boleh ada celah lewat API langsung.
- Pemulihan MFA memakai recovery codes sekali pakai. Kode ditampilkan saat dibuat, disimpan sebagai hash, dan konsumsi kode atomik. Penerbitan ulang membatalkan set lama; regenerasi memerlukan recent password + MFA maksimal 5 menit sesuai kontrak auth.
- Reset password melalui email tidak otomatis menghapus MFA. Recovery codes bukan pengganti pemeriksaan password. Kehilangan semua faktor memakai prosedur operator server terverifikasi, authorization CLI sekali pakai dan re-enrollment sesuai Technical Decisions; admin organisasi tidak dapat bypass MFA.
- Pengguna dapat melihat dan mencabut sesi login perangkat lain. Pencabutan juga menghentikan akses WS yang terkait; sesi login berbeda dari tracking session.
- Faktor kedua memakai TOTP; sesi idle 30 menit/absolute 12 jam; recent authentication 5 menit. Sesi cookie HttpOnly + CSRF dan recovery mengikuti Technical Decisions.

Kriteria penerimaan tambahan:

- `AC-AUTH-PORT-01`: akun berhak istimewa tanpa MFA yang sudah diverifikasi tidak dapat menggunakan hak tersebut melalui portal maupun API; akun dengan MFA aktif tidak mendapat sesi penuh hanya dengan password.
- `AC-AUTH-PORT-02`: tautan undangan/reset kedaluwarsa, dicabut, atau sudah dipakai ditolak; akun/tenant/peran tidak dapat diganti melalui manipulasi tautan.
- `AC-AUTH-PORT-03`: satu recovery code hanya dapat digunakan sekali, termasuk pada request concurrent; kode lama ditolak setelah regenerasi.
- `AC-AUTH-PORT-04`: reset password tidak mematikan MFA; perubahan privilege dan pemulihan akun mencabut/memperbarui sesi sesuai kebijakan auth.
- `AC-AUTH-PORT-05`: pencabutan sesi perangkat lain mengakhiri akses REST/WS terkait tanpa menutup tracking session.

## 4. Alur registrasi dan provisioning device

1. Pengguna login dan memilih konteks organisasi yang membership-nya sudah diverifikasi.
2. Portal mengirim registrasi device; backend menetapkan organisasi, UUID `device_id` tetap, dan default `session_only`.
3. Pengguna dengan izin kredensial menerbitkan token melalui endpoint rotasi; satu token ingestion aktif per device pada MVP.
4. Portal menampilkan secret sekali untuk disalin ke konfigurasi client. Halaman detail selanjutnya hanya memperlihatkan ID dan metadata kredensial, bukan secret lama.
5. Token hanya mengizinkan ingestion device/tenant tersebut. `device_id` bukan secret dan tidak cukup untuk autentikasi, akses histori, atau login portal.
6. Target serta session yang sesuai dibuat sebelum posisi dapat tersimpan pada `session_only`. Portal membedakan “device terdaftar”, “token tersedia”, dan “sesi tracking aktif”.
7. Rotasi mencabut token lama secara atomik. Jika response secret hilang, lakukan rotasi lagi; backend tidak menyediakan endpoint pengambilan ulang secret.

Token tidak boleh masuk URL, log, analytics, atau persistent browser storage. Menutup tampilan secret menghapusnya dari state UI. Penonaktifan device menolak ingestion berikutnya tanpa menghapus histori atau otomatis mengakhiri tracking session. Permintaan yang sudah bertransaksi mengikuti batas konsistensi yang dirinci dalam kontrak backend. Penonaktifan mencabut token; aktivasi kembali memerlukan penerbitan token baru, tidak menghidupkan token lama.

## 5. Peta, histori, dan privasi

Skala awal: 2 organisasi dan sekitar 100 device total, digunakan pemilik proyek. Jumlah tersebut menjadi skenario pengujian portal, bukan batas hard-coded atau jaminan kapasitas. Pengiriman posisi adaptif berdasarkan pergerakan, bukan interval tetap. Durasi sesi dan volume harian/peak masih perlu diukur. Heartbeat tanpa koordinat termasuk MVP, dengan default disepakati 60 detik jika tidak ada ingestion yang berhasil dan offline ketika umur kontak mencapai 180 detik. Portal menampilkan konektivitas, waktu kontak terakhir, usia posisi dan status session secara terpisah: device dapat online dengan posisi stale atau belum ada posisi. Backend menyediakan field/event sesuai API Contract §7.4. Heartbeat tidak menggeser marker peta atau menambah histori.

- Peta monitoring target hanya memakai sesi aktif; device dapat memperlihatkan lokasi terakhir sesuai kontrak device. Label keduanya berbeda agar posisi sesi lama tidak dianggap tracking target aktif.
- Histori device dapat melintasi beberapa penugasan; histori session dibatasi interval `[started_at, ended_at)`. Pemeriksaan izin berlaku pada query dan subscription, bukan hanya daftar pilihan portal.
- Retensi satu tahun tidak berarti seluruh data diambil sekaligus. Rentang waktu dan jumlah titik per request dibatasi; default rentang 24 jam/max 31 hari per query, page maksimal 1000; peta maksimal 10.000 titik per tampilan dengan label parsial.
- Jika yang ditampilkan hanya sebagian halaman atau titik yang disederhanakan, portal harus menyatakannya. Data asli tetap tersedia melalui pagination yang diizinkan; simplifikasi visual tidak mengubah data sumber.
- Simpan UTC; tampilkan timezone filter dan timestamp secara eksplisit. Input waktu dikonversi secara konsisten sebelum query.
- Subscribe WS lalu ambil snapshot REST; rekonsiliasi berdasarkan identitas objek/sesi dan urutan posisi agar respons snapshot lama tidak menimpa event baru. Setelah reconnect ambil snapshot ulang.
- Pergantian organisasi/logout membersihkan state lokasi, query cache dan subscription lama. Pencabutan membership/grant memutus akses pada backend, termasuk WS terbuka.
- Pemilihan penyedia peta harus mempertimbangkan biaya, lisensi, attribution dan data viewport/tile yang terlihat oleh pihak ketiga. Jangan mengirim histori atau token kepada penyedia peta. Provider MapTiler Cloud ditetapkan, sesuai Technical Decisions; tile/style request mengungkap area viewport dan IP kepada provider.

## 6. Dependensi backend dan API

API tracking yang ada digunakan untuk targets/devices/sessions, posisi, WS tickets dan ingestion. Kontrak tambahan ditetapkan pada [Management API](./gps-tracking-management-api.md), mencakup:

- Login/logout, sesi login, recovery/perubahan password dan mekanisme autentikasi browser.
- Organisasi, membership, peran, grant dan pemilihan organisasi aktif.
- Anggota dapat membaca effective grant miliknya sendiri agar portal menyembunyikan aksi per objek dengan tepat; hanya admin organisasi yang dapat membaca grant anggota lain.
- Status aktif device, aktivasi/penonaktifan dan metadata kredensial tanpa secret.
- Scope terpisah `devices:credentials:write` untuk penerbitan/rotasi token.
- Query audit terfilter tenant dengan akses admin; kebijakan pencatatan pembacaan lokasi.

Path dan schema perencanaan mengikuti Management API; OpenAPI machine-readable merupakan deliverable implementasi. Portal memakai cookie sesi HttpOnly + CSRF. Portal dan API memakai aturan akses yang sama, tanpa bypass database frontend.

## 7. Tahapan implementasi dan acceptance

Portal mulai dirancang bersamaan dengan auth, lalu diimplementasikan bertahap setelah kontrak yang dibutuhkan tersedia:

1. Shell portal, login dan organisasi aktif; pengelolaan anggota/izin.
2. Target/device dan provisioning token; start/end session.
3. Peta live, histori dengan pagination dan tampilan keadaan kosong/error.
4. Audit, pemeriksaan akses dan pengujian end-to-end untuk rilis MVP.

Kriteria penerimaan:

- `AC-PORT-01`: admin mendaftarkan device, menerbitkan token sekali, membuat session dan melihat posisi HTTP yang tersimpan di peta/histori.
- `AC-PORT-02`: token lama ditolak setelah rotasi; token device ditolak untuk login, management dan membaca histori.
- `AC-PORT-03`: tanpa sesi yang sesuai, posisi ditolak `OUTSIDE_SESSION`; portal menjelaskan kondisi ini.
- `AC-PORT-04`: viewer tidak dapat write/rotate meskipun memanggil API langsung; operator tanpa izin kredensial juga ditolak.
- `AC-PORT-05`: pergantian organisasi, akses URL langsung, cursor lama dan WS tidak membocorkan data organisasi lain.
- `AC-PORT-06`: histori session tidak mencakup periode target lain; akses target tidak membuka endpoint histori device tanpa grant.
- `AC-PORT-07`: snapshot/event out-of-order, reconnect dan data offline tidak menghasilkan tampilan lokasi aktif yang salah. Heartbeat menjaga device online tanpa mengubah usia/marker posisi; timeout mengganti status konektivitas tanpa menutup session.
- `AC-PORT-08`: posisi kedaluwarsa tidak muncul; filter timezone dan pagination konsisten; histori parsial tidak ditampilkan seolah lengkap.
- `AC-PORT-09`: device dinonaktifkan menolak kiriman berikutnya; histori yang belum kedaluwarsa tetap dapat dibaca oleh pihak berwenang.

## 8. Keputusan teknis dan langkah implementasi

Keputusan yang sebelumnya terbuka telah ditetapkan melalui delegasi pengguna: undangan 72 jam/SMTP/CLI bootstrap, tenant per tab yang diverifikasi server, TOTP, cookie sesi HttpOnly + CSRF, frontend TypeScript dan library inti, MapLibre + MapTiler. Rincian: [Technical Decisions](./gps-tracking-technical-decisions.md), [Management API](./gps-tracking-management-api.md), [Database & Operations](./gps-tracking-database-operations.md).

Pekerjaan selanjutnya mengikuti [Implementation Plan](./gps-tracking-implementation-plan.md). Akun/key provider dan kapasitas nyata diperlukan saat deployment; implementasi, OpenAPI, migrations dan pengujian belum dinyatakan selesai.
