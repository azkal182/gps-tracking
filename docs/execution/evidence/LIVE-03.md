# LIVE-03 — Rekonsiliasi event dan snapshot

Status: `done`
Tanggal: 2026-10-19

## Implementasi

- Event posisi ingestion memakai posisi latest yang benar-benar tersimpan setelah transaksi commit. ID posisi dikirim sebagai decimal string; timestamp `recorded_at`/`received_at` dinormalisasi ke UTC RFC3339 `Z`, bukan format teks default PostgreSQL.
- Context `target_id`/`session_id` untuk `position.updated` kini hanya dipasang bila posisi latest berada pada sesi yang masih aktif ketika proyeksi dibuat. Posisi backfill yang termasuk sesi historis tetap dapat menjadi latest milik device, tetapi tidak salah diproyeksikan sebagai posisi target aktif.
- Saat mengirim proyeksi target `position.updated` atau `position.status_changed`, WebSocket memeriksa lagi sesi, target, device, interval posisi dan status aktif. Ini menutup race ketika sesi selesai/dialihkan setelah event dibuat tetapi sebelum socket mengirimkannya. Event `session.ended` tetap dapat dikirim setelah session berakhir.
- Device location snapshot kini menyertakan `tenant_id` dan `observed_at` sesuai schema OpenAPI. Target snapshot sudah membawa `observed_at`; snapshot dan event sama-sama membawa position ID string agar konsumen dapat menjaga ordering posisi berdasarkan `(recorded_at, id)`.
- OpenAPI `WsPosition` sekarang mewajibkan ID posisi dan menggunakan nama `speed_mps`, konsisten dengan API tracking. Contoh contract diperbarui.

## Files

- `apps/server/src/tracking/ingestion.rs`
- `apps/server/src/tracking/realtime.rs`
- `apps/server/src/tracking/history.rs`
- `apps/server/src/tests/live.rs`
- `apps/server/src/tests/mod.rs`
- `contracts/openapi.yaml`
- `docs/gps-tracking-api-contract.md`
- `apps/portal/src/api/generated.ts` (dihasilkan oleh command contract; tidak diedit manual)

## Evidence dan validasi

- `cargo fmt --all -- --check` — lulus.
- `cargo clippy --workspace --all-targets -- -D warnings` — lulus.
- `cargo test --workspace` — lulus: 13 passed, 14 DB tests ignored tanpa `GPS_TEST_DATABASE_URL`.
- `GPS_TEST_DATABASE_URL=<disposable PostGIS> cargo test -p server --lib live03_snapshot_race_duplicate_backfill_and_target_reassignment -- --ignored --test-threads=1` — lulus 1/1 setelah perubahan akhir. Fixture menguji event posisi setelah row commit, start/end event setelah perubahan session commit, duplicate tidak memancarkan event, backfill lebih lama tidak memundurkan latest, snapshot lama tidak menimpa event dengan posisi lebih baru, pengiriman timestamp/ID schema, reassignment device ke target baru, dan suppression proyeksi sesi target yang sudah ended.
- `GPS_TEST_DATABASE_URL=<disposable PostGIS> cargo test --workspace -- --ignored --test-threads=1` — lulus pada suite PostGIS penuh: 14/14 server integration tests dan 4/4 storage/migration integration tests, termasuk AC-09/11 melalui cakupan WS LIVE-02.
- `pnpm contracts:check` — lulus; OpenAPI valid dan generated TypeScript tidak drift.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — lulus: 11 tests.
- `python3 scripts/project_tracker.py validate`, `render`, `next`, dan `git diff --check` — lulus setelah backlog diperbarui.

## Batasan

- Realtime tetap best-effort dan process-local sesuai ADR MVP; reconnect harus resubscribe lalu mengambil REST snapshot. Tidak ada durable replay atau exactly-once delivery.
- Backend menyediakan ordering metadata, tetapi aturan merge state harus diterapkan konsumen: cek scope/session, urutkan posisi dengan `(recorded_at, id)` dan jangan mengganti posisi yang lebih baru dengan snapshot/event lama. UI portal belum dibangun.
- Load benchmark 100 device pada dua organisasi belum dijalankan.

## Handoff

LIVE-03 memenuhi acceptance. Task berikutnya ditentukan oleh `python3 scripts/project_tracker.py next`.
