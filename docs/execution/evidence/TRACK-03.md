# TRACK-03 — Latest/history repositories dan expiry

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Menambahkan endpoint snapshot `GET /v1/devices/{id}/location` dan `GET /v1/targets/{id}/location`, serta history device dan session di `GET /v1/devices/{id}/positions` dan `GET /v1/sessions/{session_id}/positions`.
- Device latest memakai posisi tersimpan non-expired terbaru; target latest hanya membaca posisi yang berada dalam sesi aktif target. Status tracking, konektivitas, freshness posisi dan `observed_at` dihitung terpisah. Snapshot session dikunci `FOR SHARE` selama pembacaan agar end-session tidak mengubah interval di tengah snapshot.
- Histori memakai `recorded_at` inklusif/eksklusif, batas rentang 31 hari, default 24 jam, limit 1–1000, filter `expires_at > clock_timestamp()`, dan urutan stabil `(recorded_at DESC, id DESC)`. Cursor HMAC terikat tenant, resource, rentang waktu dan urutan; permintaan lintas tenant/resource atau batas waktu yang berubah ditolak.
- Histori session memegang transaksi yang sama untuk lookup/lock session, permission target/device, dan query interval `[started_at, ended_at)`. Permission portal memeriksa grant objek; application token memerlukan `positions:read`, allowlist resource dan grant pemilik terkini. Migration 013 menambahkan fungsi permission posisi yang khusus dan terpisah dari scope session.
- Respons lokasi/history memakai `Cache-Control: no-store`; ID posisi diserialisasi sebagai string agar BIGINT aman untuk klien.

## Requirement trace

`AC-07`, query/latest/history di kontrak API §4.4–5.5 dan §6, aturan temporal session `[start,end)`, expiry satu tahun kalender UTC serta otorisasi tenant/object dan scope `positions:read`.

## File

`apps/server/src/tracking/history.rs`, `apps/server/src/tracking/mod.rs`, `apps/server/src/tracking/sessions.rs`, `apps/server/src/lib.rs`, `apps/server/src/tests/tracking.rs`, `migrations/202610190013_application_token_position_permission.sql`, dan dokumen evidence ini.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 9 unit tests; tes DB yang memerlukan Postgres ditandai ignored secara default |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server track03_latest_history_cursor_expiry_and_session_visibility -- --ignored --nocapture` | Lulus di PostGIS 17/3.5 disposable: latest target tidak membocorkan posisi sesi lama; latest device, ties ID, pagination, cursor batas waktu/resource/tenant, sesi end-exclusive, expiry dan `positions:read` vs scope session |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p tracking-storage --test database_migrations tracking_domain_constraints_retention_and_concurrency -- --ignored --nocapture` | Lulus: constraint domain, concurrency, expiry tahun kalender UTC termasuk normalisasi leap-day |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tracker tests |
| `python3 scripts/project_tracker.py validate` | Lulus |
| `python3 scripts/project_tracker.py render` | Lulus |

Histori API membatasi rentang maksimum 31 hari, sehingga posisi lebih lama dari retensi tidak bisa diminta melalui endpoint untuk mengisolasi predicate expiry; storage integration membuktikan expiry kalender/leap-day dan source query menerapkan filter eksplisit. DB pengujian adalah container lokal tanpa volume dan hanya berisi fixture sintetis; container dihentikan dan dihapus setelah tes. Tidak ada credential koneksi atau koordinat nyata yang dicatat.

## Batas verifikasi

Tes handler memanggil Axum handlers langsung, bukan HTTP socket/E2E browser. Belum ada benchmark histori berskala besar. Task berikutnya: `LIVE-01` (heartbeat dan status terpisah).
