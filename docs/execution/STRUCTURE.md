# Struktur source dan refactor bertahap

Struktur mengikuti kepemilikan domain dan batas runtime, bukan ambang LOC mekanis. Refactor dilakukan bertahap tanpa menggabungkan perubahan perilaku/kontrak. Folder baru dipakai ketika domain memiliki beberapa modul; file generated dan fixture tetap pada lokasi generator/pemiliknya.

## Backend Rust

`apps/server/src/lib.rs` adalah crate facade dan composition root: config/state bersama, health/readiness, router dan middleware lintas domain. Implementasi domain berada di modul root yang eksplisit:

- `resources.rs`: target, device, credential dan validasi/paginasi resource.
- `organizations.rs`: organisasi, invitation, membership, grants dan otorisasi object bersama.
- `auth_routes.rs` + `mfa.rs`: sesi login, password/recovery, endpoint MFA dan primitive TOTP/enkripsi. Jika domain auth memperoleh beberapa berkas implementasi tambahan, gabungkan di `auth/` tanpa mengubah lifecycle boundary.
- `email.rs`: adapter email/outbox.
- `tracking/` menjadi domain baru untuk session tracking, ingestion, heartbeat, history dan realtime saat P07/P08 dibangun; jangan mencampur lifecycle login.
- `tests/`: test server dikelompokkan berdasarkan domain (`auth`, `organizations`, `devices`, `unit`). Tes lintas crate tetap pada `crates/<domain>/tests/`.

Jangan mengembalikan handler ke `lib.rs` dan jangan membuat kantong generik `utils`, `common`, atau `handlers` sebagai tujuan akhir. Helper lintas domain harus berada pada domain pemilik atau API crate yang jelas. Perubahan schema/kontrak/perilaku mengikuti task dan decision terkait, bukan disamarkan sebagai refactor.

## Portal React

Saat portal domain berikutnya dikerjakan, susun `apps/portal/src/app/` untuk bootstrap/router/provider, `src/features/<domain>/` untuk halaman, komponen, hooks dan schema form domain, serta `src/shared/` untuk UI generik, API transport dan utilitas bersama. Pindahkan halaman lama saat disentuh oleh task domain agar query, form, state dan tes bergerak bersama. `src/api/generated.ts` adalah output generator kontrak: jangan edit atau pecah manual.

## Validasi

Setiap langkah menjaga URL, schema, authorization, transaksi dan response. Jalankan formatting, clippy dan test relevan. Catat secara eksplisit test yang memerlukan PostgreSQL/PostGIS jika service disposable tidak tersedia. Refactor tidak mengubah baseline atau status task produk yang sudah selesai.
