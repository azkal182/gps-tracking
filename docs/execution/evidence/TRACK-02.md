# TRACK-02 — HTTP ingestion dan canonical validation

Tanggal: 2026-10-19  
Status: selesai.

## Implementasi

- Ditambahkan `POST /v1/ingest/positions` dengan device bearer `gps_live_` saja. Credential di-resolve melalui lookup SECURITY DEFINER, payload `device_id` harus cocok dengan identitas token, lalu identitas credential diperiksa ulang setelah lock organisasi → device → target → session diperoleh.
- Request menerima 1–100 titik dan dibatasi 256 KiB. Validasi struktur mencakup UUID device, message ID opaque 1–128 karakter, koordinat finite/range, pengukuran finite dan batas satuan, RFC 3339 dengan offset, serta attributes object maksimal 4 KiB.
- Timestamp dinormalisasi UTC, optional measurements dan attributes default dinormalisasi, signed zero disamakan, serta map JSON bersifat key-order independent sebelum fingerprint SHA-256 dihitung. Duplicate diperiksa sebelum time/session policy; fingerprint sama menghasilkan `duplicate`, berbeda menghasilkan `MESSAGE_ID_CONFLICT` dan audit tanpa menyimpan koordinat atau credential.
- `session_only` hanya menyimpan point dalam interval sesi `[started_at, ended_at)`, mendukung backfill 24 jam, menolak titik di luar sesi, dan mengembalikan `TIMESTAMP_NOT_READY` dengan retry millisecond untuk future timestamp sampai 2 menit. Future lebih jauh dan point terlalu lama ditolak. `always` dapat menyimpan tanpa sesi setelah batas waktu dipenuhi.
- Batch diproses dalam satu transaksi: semantic rejects tetap per-item, sementara kegagalan DB menghasilkan 500 dan rollback semua insert. Dedupe race diserialkan dengan device lock dan tetap dilindungi unique constraint. Batch terautentikasi yang selesai memperbarui `last_contact_at` secara monotonik; error transaksi tidak mengubahnya.
- OpenAPI sudah menyatakan `message_id` sebagai string 1–128 karakter, sedangkan migration lama membatasi kolom ke UUID. Migration 012 mengubah kolom menjadi TEXT sambil mempertahankan unique `(device_id,message_id)` dan menambah constraint panjang.

## Requirement trace

`FR-ING-01..08`, `BR-02`, `BR-03`, `BR-09..11`, `BR-13`, `AC-03`, `AC-04`, `AC-05`, `AC-06`, `AC-08`, dan serialisasi ingestion/session pada `AC-12`.

## File

`apps/server/src/tracking/ingestion.rs`, `apps/server/src/tracking/mod.rs`, `apps/server/src/lib.rs`, `apps/server/src/tests/tracking.rs`, `migrations/202610190012_position_message_id_text.sql`, `crates/tracking-storage/tests/database_migrations.rs`, dan dokumen evidence ini.

## Verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --check` | Lulus |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lulus |
| `cargo test --workspace` | Lulus: 9 unit tests; 14 integration tests berpenanda ignored memerlukan database eksternal khusus |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tracker tests |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p server track -- --ignored --test-threads=1` | Lulus: TRACK-01 regression dan TRACK-02 integration. Menguji mixed stored/duplicate/rejected, opaque ID, canonical hash untuk key order dan offset waktu, wrong-device token, invalid measurement, duplicate/conflict, concurrent duplicate, `always`, retryable future, late position dalam sesi berakhir, end boundary eksklusif, outside-session, dan atomic rollback setelah synthetic DB failure |
| `GPS_TEST_DATABASE_URL=<PostGIS disposable> cargo test -p tracking-storage tracking_domain_constraints_retention_and_concurrency -- --ignored --test-threads=1` | Lulus: migrasi/constraint/retensi tracking termasuk schema `message_id` TEXT dan unique per device |

Integration memakai satu container `postgis/postgis:17-3.5-alpine` lokal tanpa volume dan data sintetis; container dihapus setelah tes. Tidak ada data produksi yang dipakai.

## Batas verifikasi

Rate limiting ingestion, benchmark target p95, dan HTTP end-to-end melalui socket/server belum diuji di task ini. DB integration memanggil handler Axum secara langsung; route dan body-limit telah dipasang pada router. WebSocket/event publication serta heartbeat tetap berada pada task P08.
