# Baseline final perencanaan MVP — v1.0

> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.

## 1. Arti final

Keputusan produk dan teknis dalam [indeks planning](./README.md) menjadi spesifikasi yang harus dilaksanakan. Implementor tidak boleh mengganti model auth, batas tenant, aturan waktu, retensi, kontrak heartbeat atau stack tanpa mencatat perubahan desain beserta dampaknya. Bug/kontradiksi yang ditemukan harus diperbaiki melalui revisi eksplisit, bukan dianggap izin untuk memilih perilaku secara diam-diam.

Finalisasi tidak mengubah estimasi menjadi fakta empiris. Karena aplikasi belum ada, kapasitas, latency, kebutuhan disk, RPO/RTO dan kualitas keamanan hanya dapat dibuktikan melalui implementasi dan pengujian.

## 2. Klasifikasi agar tidak ada asumsi tersembunyi

| Kategori | Status | Perlakuan |
|---|---|---|
| Produk, domain, tenant, portal dan peran | Keputusan final | Implementasikan sesuai App/Portal Spec |
| Auth, MFA, tenant context, API, heartbeat, retensi | Keputusan final | Ikuti Technical Decisions dan kedua API docs |
| Frontend, peta, deployment dan backup | Keputusan final | Stack/provider dipilih; credentials layanan disediakan saat deployment |
| Default timeout, TTL, pagination, limits | Keputusan final konfigurabel | Perubahan konfigurasi tervalidasi; bukan angka wajib dari standar eksternal |
| 100 device total/2 organisasi | Skenario sizing baseline | Distribusi antarorganisasi tidak memengaruhi schema; bukan hard limit |
| Laju movement, durasi sesi dan volume riil | Belum terukur | Gunakan skenario sintetis dan ukur pilot; jangan klaim volume pasti |
| Throughput, disk, latency dan RPO/RTO | Target pengujian | Release gate, bukan hasil yang sudah dicapai |
| SQL migrations, OpenAPI/WS schemas, source code | Deliverable implementasi | Harus dibuat/diuji, bukan dianggap tersedia karena planning final |
| Domain, host, SMTP/tile/backup credentials | Input deployment | Tidak menghalangi local development; belum ada pembelian atau konfigurasi akun |
| MQTT, binary hardware, WebAuthn, analytics | Di luar MVP/fase lanjut | Tidak menghambat finalisasi MVP |

## 3. Penutupan ambigu dari audit final

- Public hard-delete target/device/session tidak tersedia; pencabutan membership, sesi login dan kredensial tetap tersedia.
- `external_ref` target non-null wajib unik per tenant; batas name/metadata/identifier ditetapkan pada Tracking API.
- Pagination daftar default 50/max 100; posisi default 100/max 1000, rentang query max 31 hari.
- `Idempotency-Key` wajib pada POST tracking session, TTL 24h dan scope tenant/principal/route; replay memeriksa izin terkini.
- Response resource tenant menyertakan tenant_id; contoh tracking diperbarui. Location snapshot memakai observed_at dan position ID.
- Token device aktif sampai revoked/rotated/disabled; application token 90 hari. Cookie portal idle 30 menit/absolute 12 jam.
- Frontend library inti sudah dipilih; Zustand kondisional merupakan keputusan menunda dependency sampai dibutuhkan, bukan keputusan produk terbuka.
- Cookie portal, bearer integrasi dan bearer device berbeda; tidak ada refresh JWT endpoint yang diam-diam dibutuhkan.

## 4. Kriteria mulai dan rilis

Tidak ada keputusan produk tambahan yang harus ditanyakan sebelum memulai milestone 1. Ikuti [Implementation Plan](./gps-tracking-implementation-plan.md). Source, OpenAPI dan migrations perlu saling diverifikasi sebelum modul dinyatakan selesai.

Sebelum pilot production: seluruh acceptance P0 lulus, negative authorization dan recovery diuji, load/storage forecast diukur, restore drill memenuhi target, expiry/purge bekerja, dan konfigurasi layanan tersedia. Jika salah satu gagal, perbaiki implementasi atau revisi desain secara eksplisit; label dokumen final tidak mengizinkan melewati gate.

## 5. Bukti audit dokumen

Pemeriksaan mencakup konsistensi keputusan lintas dokumen, pembacaan titik ambigu, field/status yang telah berubah, parse contoh JSON, tautan lokal, dan keseimbangan fenced blocks. Audit dokumen tidak membuktikan correctness migration, keamanan runtime, keberhasilan integrasi provider atau performa database; semuanya tercakup dalam release gates di atas.
