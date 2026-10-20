# LIVE-01 — Heartbeat dan status terpisah

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Menambahkan `POST /v1/ingest/heartbeat` untuk device bearer `gps_live_` dengan body JSON kosong saja. Field tambahan, koordinat, timestamp dan device ID ditolak oleh schema; request memakai body limit 1 KiB.
- Lookup kredensial memeriksa organisasi aktif, device enabled dan token belum dicabut. Setelah autentikasi, handler memakai bucket token per device (3 burst, refill 6/menit), mengembalikan `429` dan `Retry-After` saat penuh, lalu mengunci organisasi/device dan memeriksa ulang kredensial sebelum update.
- `last_contact_at` ditulis dengan waktu server dan `GREATEST` agar monotonik. Transaksi tidak membaca/mengubah tracking session, menambah position atau mengubah timestamp posisi. Kegagalan autentikasi/struktur tidak menyentuh waktu kontak.
- Heartbeat sukses mengembalikan `connectivity_status: online`, `last_contact_at`, `X-Request-Id` dan `Cache-Control: no-store`.
- Snapshot device/target memakai ambang online/offline dan fresh/stale yang dapat dikonfigurasi melalui `DEVICE_OFFLINE_TIMEOUT_SECONDS` (default 180) dan `POSITION_FRESHNESS_SECONDS` (default 60). Nilai non-positif/malformed menggagalkan validasi konfigurasi; `.env.example` mencantumkan nilai awal.
- Tidak ada baris histori heartbeat. Timeout event/worker dan WebSocket tetap scope LIVE-02; REST snapshot sudah menghitung status dari timestamp tersimpan.

## Requirement trace

`FR-HB-01..02`, `AC-13`, `AC-14`, API Contract §7.4, cadence rekomendasi 60 detik, timeout konektivitas 180 detik, freshness posisi 60 detik, dan database operations heartbeat 6/menit dengan burst 3.

## File

`apps/server/src/tracking/heartbeat.rs`, `apps/server/src/tracking/mod.rs`, `apps/server/src/tracking/history.rs`, `apps/server/src/lib.rs`, `apps/server/src/main.rs`, `apps/server/src/tests/tracking.rs`, `apps/server/src/tests/unit.rs`, `.env.example`, dan dokumen evidence ini.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 12 unit tests; 12 tes DB ditandai ignored secara default |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server live01_heartbeat_contact_status_freshness_and_lifecycle -- --ignored --nocapture` | Lulus di PostGIS 17/3.5 disposable: tanpa sesi, unknown/no_position, kontak sukses, heartbeat tanpa position/session, stale-online, 59/60 detik freshness, 179/180 detik connectivity, override threshold runtime, timestamp monotonik/future, concurrent heartbeat, batch semantik ditolak tetap kontak, device disable, credential revoke, dan burst/Retry-After |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tracker tests |
| `python3 scripts/project_tracker.py validate` | Lulus |
| `python3 scripts/project_tracker.py render` | Lulus |

Tes database memakai container lokal tanpa volume dan fixture sintetis; container dihapus sesudah tes.

## Batas verifikasi

Rate limiter disimpan in-memory per instance, sesuai baseline operasional satu instance; horizontal scaling memerlukan limiter bersama. Worker timeout/event dan restart reconciliation belum dikerjakan di LIVE-01 dan menjadi LIVE-02/LIVE-03. Handler diuji langsung, bukan melalui HTTP socket.
