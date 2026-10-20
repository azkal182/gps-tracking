# Portal — pilihan frontend dan rekomendasi library

> **Tanggal:** 19 Oktober 2026  
> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.
> **Terkait:** [Portal Spec](./gps-tracking-portal-spec.md), [App Spec](./gps-tracking-app-spec.md), [API Contract](./gps-tracking-api-contract.md)

## 1. Fondasi

Portal berada di `apps/portal`, backend Rust di `apps/server`, dalam satu repository. Keputusan menggunakan TypeScript strict, React Router untuk navigasi, dan TanStack Query untuk server state. Vite menghasilkan aset statis; rancangan deployment awal menyertakannya bersama server Rust pada satu origin. Detail build dan routing fallback wajib memastikan `/v1/*` tidak pernah jatuh ke HTML portal.

shadcn/ui menyediakan kode komponen yang menjadi bagian source proyek; perubahan dan aksesibilitas komponen tetap menjadi tanggung jawab proyek. Tambahkan hanya komponen yang dibutuhkan. Gunakan integrasi Tailwind/Vite resmi saat scaffold; versi aktual harus diperiksa kompatibilitasnya dan dikunci pada lockfile, bukan menyalin versi dari contoh lama.

## 2. Library pendukung yang ditetapkan

| Kebutuhan | Pilihan | Kapan dan alasan |
|---|---|---|
| Routing | React Router | Sejak shell portal; nested layouts, halaman organisasi/device/session; URL menyimpan filter non-secret yang layak dibagikan |
| Server state | `@tanstack/react-query` | Sejak integrasi API; cache, mutations dan invalidation. Key memuat tenant dan konteks resource/filter |
| Form | `react-hook-form` + `@hookform/resolvers` | Login, undangan, registrasi device, metadata dan start session; field state, errors, dirty/submitting |
| Validasi | `zod` | Schema form dan boundary runtime yang relevan, misalnya event WS; bukan pengganti validasi backend |
| Typed REST client | `openapi-typescript` + `openapi-fetch` | Setelah OpenAPI tersedia; generate tipe dan gunakan fetch client bertipe, hindari tipe request/response ditulis ulang |
| Tabel | `@tanstack/react-table` + komponen shadcn Table | Saat membangun daftar device, sesi dan audit; pagination/filter server-side sesuai kontrak |
| Global UI state | `zustand` | Bersyarat: selection/follow-mode peta atau state UI lintas komponen yang sulit ditangani lokal; tidak wajib di scaffold |
| Peta | `maplibre-gl` | Renderer dipilih untuk marker/layer/jalur; provider MapTiler Cloud; akun/key dan paket disediakan saat deployment |
| Unit/component testing | Vitest, React Testing Library | Saat ada perilaku interaktif untuk diuji; library testing tambahan diverifikasi saat setup |
| End-to-end testing | Playwright | Login/MFA, grants, organisasi aktif, provisioning, peta/histori dan session lifecycle |

Utilitas ikon/toast/class merging mengikuti komponen dan preset shadcn yang dipilih, bukan menambah beberapa alternatif untuk fungsi sama. Belum perlu Axios di samping openapi-fetch, Socket.IO di atas kontrak WebSocket biasa, Redux di samping Query/local state, atau library chart sebelum ada kebutuhan analitik yang disepakati.

## 3. Batas tanggung jawab state

- TanStack Query: data server seperti device, membership, session, posisi dan histori. Jangan duplikasi seluruh koleksi ke Zustand.
- React Hook Form: nilai field, validation error, dirty state dan status submit. Error backend dipetakan ke field atau form-level error.
- URL search params: filter waktu/device/session yang layak di-bookmark; jangan masukkan token, recovery code atau koordinat mentah. State URL tetap divalidasi dan tidak memberi hak akses.
- React local state: dialog, tab dan interaksi lokal. Zustand hanya saat state UI memang melintasi banyak komponen; reset saat logout/pergantian organisasi.
- Cache lokasi tidak dipersist ke localStorage/IndexedDB secara default. Token device yang ditampilkan sekali tidak masuk Query cache/persisted store/devtools/logging.
- Pergantian organisasi membatalkan request lama, membersihkan cache sensitif, menutup WS lama dan mencegah response yang terlambat masuk ke konteks baru. Tenant pada key bukan pengganti authorization server.
- WebSocket dikelola modul terpusat dengan reconnect/backoff, resubscribe dan snapshot. Integrasikan perubahan dengan Query secara terukur; jangan refetch seluruh histori pada setiap event heartbeat.

## 4. Validasi dan kontrak

TypeScript/OpenAPI memberi pemeriksaan tipe saat build, bukan validasi runtime otomatis. Zod memberi validasi runtime untuk input/form dan event yang diproses frontend. Rust tetap memeriksa seluruh payload, tenant, izin dan invariant bisnis. Schema form boleh berbeda karena format input UI, tetapi aturan yang diduplikasi harus konsisten dengan API dan diuji pada boundary penting.

Periksa kompatibilitas Zod/resolver/React Hook Form pada versi yang dipilih saat scaffold. Jangan menyalin contoh dokumentasi dengan asumsi semua contoh menggunakan major version sama. OpenAPI REST tidak otomatis mencakup kontrak WS; schema event perlu sumber kontrak eksplisit.

Cursor pagination backend tidak otomatis mendukung nomor halaman acak atau sorting kolom arbitrer. Tabel portal menggunakan next/previous cursor dan hanya filter/sort yang didukung API. Histori satu tahun tidak diambil seluruhnya untuk client-side sorting.

## 5. Alasan pilihan dan batas implementasi

React Hook Form + Zod ditetapkan sejak awal karena kebutuhan form management jelas dan integrasinya didokumentasikan shadcn. TanStack Form merupakan alternatif yang didukung shadcn, tetapi tidak ada kebutuhan saat ini yang mengharuskan dua library form sekaligus.

Zustand bermanfaat untuk UI state bersama, tetapi Query, form state, URL dan state lokal sudah menangani sebagian besar kebutuhan. Tambahkan ketika modul peta menunjukkan kebutuhan konkret. MapLibre adalah renderer, bukan layanan penyedia peta; penggunaan library ini tidak berarti tile hosting gratis atau tersedia untuk produksi.

Keputusan lanjut: MapLibre + MapTiler, cookie sesi HttpOnly + CSRF, MFA TOTP, pnpm. Rincian berlaku pada [Technical Decisions](./gps-tracking-technical-decisions.md). Versi kompatibel dan perintah build/test diwujudkan saat scaffold. Pilihan frontend tidak mengubah cakupan backend, kontrol tenant, heartbeat atau retensi.

## 6. Sumber primer yang ditinjau

- [shadcn/ui — Vite dan Tailwind](https://ui.shadcn.com/docs/installation/vite)
- [shadcn/ui — React Hook Form dan Zod](https://ui.shadcn.com/docs/forms/react-hook-form)
- [shadcn/ui — pilihan form](https://ui.shadcn.com/docs/forms)
- [Zod — parsing dan inferred types](https://zod.dev/basics)
- [React Hook Form resolvers](https://github.com/react-hook-form/resolvers)
- [Zustand — introduction](https://zustand.docs.pmnd.rs/learn/getting-started/introduction)
- [React Router — modes](https://reactrouter.com/start/modes)
- [TanStack Query — overview](https://tanstack.com/query/latest/docs/framework/react/overview)
- [shadcn/ui — data table](https://ui.shadcn.com/docs/components/data-table)
- [OpenAPI TypeScript — openapi-fetch](https://openapi-ts.dev/openapi-fetch/)
- [MapLibre GL JS](https://maplibre.org/maplibre-gl-js/docs/)
- [Vitest](https://vitest.dev/guide/)
- [Playwright](https://playwright.dev/docs/intro)
