# OPS-03 — Backup/restore dan rollback drill

Tanggal: 2026-10-20

## Perubahan

- Menambahkan `ops/backup/pgbackrest.conf.example` tanpa cipher secret dan
  `ops/backup/restore-runbook.md` untuk backup full/incremental, WAL archive,
  retensi, pemulihan key, PITR terisolasi, replay tombstone, purge, invalidasi
  auth, tenant/last-admin checks, target RPO/RTO dan kompatibilitas rollback.
- Drill memakai container PostGIS 17 disposable, repository pada Docker named
  volume terpisah dari PGDATA, dan pgBackRest 2.59.1. Repository terenkripsi
  AES-256-CBC; passphrase random hanya ada di config mode 0600 dalam container,
  tidak dicetak atau disalin ke repository.
- `stanza-create`, `check`, satu full, dua incremental, `info`, WAL push dan PITR
  berhasil. Restore dilakukan ke data directory terpisah dan berjalan pada port
  54332 sementara sumber tetap aktif. Marker transaksi sesudah target tidak ada
  di database hasil restore.
- Synthetic-only cleanup setelah PITR: replay tombstone menghapus satu posisi,
  role `gps_tracking_retention_worker` menghapus satu posisi expired; sesi auth,
  account token, application token dan device credential aktif menjadi nol.
  Tenant tetap memiliki active organization admin; hitungan/verifikasi tidak
  membaca atau menyalin koordinat.

## Bukti

- `GPS_TEST_DATABASE_URL=postgres://postgres:<disposable>@127.0.0.1:54331/tracker_test cargo test -p tracking-storage --test database_migrations fresh_upgrade_uniqueness_missing_tenant_and_atomic_consumption -- --ignored --exact` — lulus, migrasi lengkap dan skema aplikasi diterapkan ke DB disposable.
- `pgbackrest --stanza=ops03-drill stanza-create`, `check`, `--type=full backup`, dua `--type=incr backup`, dan `info` — lulus; `info` menunjukkan full + dua incremental, stanza `status: ok`, cipher `aes-256-cbc`; WAL 000000010000000000000001–00000001000000000000000C.
- PITR target `2026-10-20 01:39:55.903115+00`; log restore menunjukkan backup/WAL dipulihkan, transaksi setelah target tidak dikomit saat recovery berhenti. Last completed transaction tercatat `2026-10-20 01:39:54.11635+00`; transaksi sentinel sesudah target tercatat `01:39:56.044251+00`. RPO hasil simulasi = 1.928 detik, di bawah target 5 menit.
- Dari restore invocation `01:40:09.770 UTC` sampai pemeriksaan restore siap `01:42:58.296 UTC`, RTO database + cleanup/readiness check = 168.526 detik, di bawah target 4 jam. Restore menyelesaikan 61 MB/2.087 file; pengukuran mencakup replay/purge/invalidation dan verifikasi sebelum API dibuka.
- Hasil restore sebelum dan sesudah sanitasi: sentinel pasca-target 0; posisi synthetic device sesudah tombstone dan purge 0; session, account-token, application-token, device-credential aktif 0; organisasi 2 pada DB fixture dan active organization admin 1.
- `RETENTION_DATABASE_URL=postgres://postgres:<disposable>@127.0.0.1:54331/tracker_test cargo run -p server --bin retentionctl` — lulus pada DB primer disposable, melaporkan `positions=1`, tanpa menampilkan payload lokasi.
- `cargo fmt --all -- --check` dan `cargo clippy --workspace --all-targets -- -D warnings` — lulus.
- `cargo test --workspace` — 17 lulus; 15 server integration tests dan empat storage integration tests di-ignore secara default karena membutuhkan database.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — lulus, 11 tests. `python3 scripts/project_tracker.py validate` — valid untuk 40 task; runtime evidence tetap memerlukan review manual. `render` dijalankan setelah update backlog.

## Batas dan status acceptance

- Docker named volumes dan repo berada pada host Docker lokal yang sama. Ini
  membuktikan enkripsi repository dan alur PITR, bukan ketahanan off-host, storage
  provider, rotasi/pemulihan key dari vault, jadwal mingguan/harian, alerting,
  ukuran kapasitas, atau failover produksi.
- Setelah pengukuran dicatat, container dan volume database/repository disposable
  (`gps-ops03-primary`, `gps-ops03-data`, `gps-ops03-repo`) dihapus; hanya fixture
  sintetis yang digunakan.
- RTO yang diukur mencapai kesiapan database terisolasi setelah data cleanup dan
  pre-open checks; API ingress tidak pernah dibuka. Staging wajib menambahkan
  end-to-end API readiness dan negative cross-tenant smoke tests.
- Repository ini belum memiliki image aplikasi terdahulu, jadi kompatibilitas
  binary lama belum diuji dalam drill ini. Berdasarkan keputusan EXEC-007, uji
  aktual image sebelumnya dipindahkan menjadi gate PILOT-01 (build sebelumnya
  tidak harus pernah production). Runbook OPS-03 sudah menetapkan kondisi rollback
  dan bukti yang harus dikumpulkan; ini tidak menghapus gate sebelum pilot.

## Langkah berikut

Berikutnya OPS-04 sesuai dependency backlog. PILOT-01 wajib menguji image/build
sebelumnya terhadap schema hasil migration sebelum pilot; jika belum tersedia,
buat dua kandidat dari code state berurutan dan jalankan pengujian di staging.
