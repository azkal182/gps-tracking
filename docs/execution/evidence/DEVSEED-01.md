# DEVSEED-01 — Browser seed data dan login lokal

Status: **selesai** pada 2026-10-20.

## Deliverable

- Ditambahkan `cargo run --locked -p server --bin devseed`.
- Seed mensyaratkan `APP_ENV=development`, password via `GPS_DEV_SEED_PASSWORD` (15–128 karakter sesuai login contract, bukan argumen CLI), dan `GPS_DEV_SEED_DATABASE_URL` yang host-nya loopback serta nama DB berakhiran `_dev` atau `_test`. Password tidak dicetak; DB hanya menyimpan hash Argon2id.
- Seed menerapkan migration, memakai advisory lock dan transaksi atomik, lalu idempotent berdasarkan email demo. Tidak menjalankan DELETE/update saat seed sudah ada.
- Akun `demo.admin@local.test` adalah org-admin aktif pada dua organisasi dan masuk alur login/MFA normal; MFA tidak dilewati atau dipalsukan.
- Fixture berisi dua organisasi lokal, lima target/device, lima sesi tracking aktif dan 120 titik sintetis (24/device) di area Jakarta dan Surabaya. Tidak ada token device atau data nyata.
- Panduan start DB, seed, server, Vite portal, kredensial login dan keterbatasan map provider ditambahkan ke `docs/execution/development-environment.md`.

## Verifikasi (2026-10-20)

- `cargo test -p server --bin devseed` — 1 lulus: URL remote/non-dev/non-test ditolak.
- `cargo clippy --workspace --all-targets -- -D warnings` — lulus.
- `APP_ENV=development GPS_DEV_SEED_DATABASE_URL=... GPS_DEV_SEED_PASSWORD=<synthetic> cargo run --locked -p server --bin devseed` pada PostGIS disposable loopback — lulus; migration diterapkan dan seed berhasil.
- SQL row counts setelah seed: 2 organizations, 1 user, 2 memberships, 5 targets, 5 devices, 5 sessions, 120 positions.
- Rerun CLI pada DB yang sama — lulus sebagai no-op (`Local demo seed already exists; no records changed.`).
- Pada awal implementasi belum ada `.env`/database dev. Setelah user meminta `.env` lengkap, dibuat `.env` mode 0600 dengan key/password acak stabil, URL DB loopback, retention 60 hari dan `GPS_DEV_SEED_PASSWORD`; Compose membuat volume baru `backend-tracker_postgres_data` (volume bernama tersebut sebelumnya tidak ada).
- `docker compose up -d --wait` — DB dan Mailpit healthy; seed diterapkan pada DB lokal persisten dengan hitungan 2 organisasi, 1 user, 2 membership, 5 target, 5 device, 5 sesi dan 120 posisi.
- Login melalui API dengan password dari `.env` mengembalikan HTTP 200 `mfa_enrollment_required`, membuktikan kredensial benar dan MFA normal tetap diwajibkan. Password/cookie tidak dicetak.
- Backend `/readyz` mengembalikan `{"status":"ready"}`; Vite portal mengembalikan halaman login di `http://localhost:5173/login`. Tab browser dibiarkan terbuka untuk uji manual.

## Cara memakai

Ikuti bagian “Seed untuk uji coba portal di browser” pada `docs/execution/development-environment.md`. Pilih password sendiri melalui prompt tersembunyi. Login pertama mewajibkan setup MFA. UI peta membutuhkan MapTiler key/style; portal dan histori tetap bisa dijalankan tanpa provider peta.
