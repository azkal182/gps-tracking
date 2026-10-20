# REFAC-01 — Struktur modul server dan pemisahan test

Tanggal: 2026-10-19  
Status: selesai.

## Scope

Refactor behavior-preserving sebelum kembali ke task produk DEVICE-03. Tidak ada task produk baru yang dikerjakan. DEVICE-01 dan DEVICE-02 tetap done.

## Perubahan

- Memindahkan composition root, shared config/state, health/readiness dan route assembly saja ke `apps/server/src/lib.rs` (turun dari 7.109 menjadi 418 baris).
- Memisahkan target/device/credential menjadi `apps/server/src/resources.rs`, organisasi/invitation/membership/grant ke `organizations.rs`, dan login/password/recovery/MFA/session handlers ke `auth_routes.rs`.
- Memecah test server menjadi `tests/{unit,auth,organizations,devices}.rs` dengan fixture org bersama di `tests/mod.rs`.
- Menetapkan aturan struktur backend/portal dan batas refactor di `docs/execution/STRUCTURE.md`. Portal belum dipindah karena tidak ada perubahan pada portal di scope ini; generated client tetap milik generator.
- Mempertahankan tiga fungsi otorisasi yang sebelumnya public pada crate root dengan re-export publik.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: server 9 passed, 7 ignored; migration DB 4 ignored; test lain lulus/tidak memiliki test. 11 tes memerlukan `GPS_TEST_DATABASE_URL` Postgres/PostGIS disposable. |
| `pnpm contracts:check` | Exit 0; Redocly lint valid dan client generated dibuat. |
| Regenerate `pnpm contracts:generate` dan bandingkan SHA-256 `apps/portal/src/api/generated.ts` sebelum/sesudah | Hash identik: `0a7c6fd5cc94acba8ec1dff89b0c80fc6845cbebad72658a4713919424750cbc`. |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tests. |
| `python3 scripts/project_tracker.py validate` | Lulus setelah evidence dan backlog diperbarui. |
| `python3 scripts/project_tracker.py render` | Lulus; STATUS/BACKLOG dihasilkan kembali. |

## Batas

Test integrasi Postgres/PostGIS yang ditandai ignored tidak dijalankan ulang pada refactor ini karena `GPS_TEST_DATABASE_URL` tidak disediakan. Behavior ditutup oleh compile, clippy dan unit tests yang tersedia; test khusus Postgres tetap tercatat pada evidence task sebelumnya. File-file test integrasi domain tetap lebih panjang daripada unit test karena satu skenario end-to-end memeriksa beberapa invariant; mereka kini terkelompok agar perubahan domain dapat ditinjau terpisah.

## Handoff

Lanjutkan DEVICE-03 sebagai task produk berikutnya. Sebelum P07 ingestion, pertahankan ketentuan DEVICE-02 untuk mengunci device dan memvalidasi ulang credential setelah lock diperoleh.
