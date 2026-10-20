# OPS-04 — Load dan E2E release readiness

Status: **selesai** pada 2026-10-20. Seluruh acceptance task terverifikasi dengan fixture sintetis dan readiness suites.

## Cakupan dan environment

- `scripts/ops04_load.py` menyediakan harness reproduktif untuk dua tenant dan 100 device: 1 juta posisi sintetis, heartbeat idle, steady ingestion, burst, backfill, history page, race session-end/ingestion, query plan, serta proyeksi storage.
- Harness hanya menerima database disposable baru. Token acak hanya disimpan di memori. Tidak ada data atau kredensial produksi.
- Container PostGIS `postgis/postgis:17-3.5-alpine`, Docker VM Linux (12 vCPU, RAM 7.75 GiB terlapor), dibatasi `--memory=5g --memory-swap=5g --shm-size=1g`, data directory tmpfs maksimum 4 GiB, port loopback 54331.
- Perintah migrasi: `GPS_TEST_DATABASE_URL='postgres://postgres:ops04-disposable@127.0.0.1:54331/tracker_test?sslmode=disable' cargo test -p tracking-storage --test database_migrations fresh_upgrade_uniqueness_missing_tenant_and_atomic_consumption -- --ignored --exact` — passed.
- Full load: `PYTHONPYCACHEPREFIX=/tmp/ops04-pycache python3 -u scripts/ops04_load.py --steady-seconds 120`.
- History follow-up (30 halaman, dua tenant): `PYTHONPYCACHEPREFIX=/tmp/ops04-pycache python3 -u scripts/ops04_load.py --history-only`.

## Hasil beban

| Scenario | Hasil |
|---|---|
| Seed | 1,000,000 posisi; 77.414 s; relasi posisi 515,932,160 byte; 1,000,000 row total ketika seed selesai |
| Idle heartbeat | 100/100 HTTP 200 pada 10 req/s; p50 17.18 ms, p95 375.08 ms, max 513.79 ms; jumlah posisi tidak bertambah |
| Steady ingestion | 1,200/1,200 HTTP 200 selama 119.962 s; 10.0 titik/s; p50 31.21 ms, p95 64.50 ms, max 276.26 ms |
| Burst ingestion | 6,000/6,000 HTTP 200 selama 60.073 s; 99.88 titik/s; p50 40.74 ms, p95 105.78 ms, max 206.77 ms |
| Backfill | 10/10 batch HTTP 200; 1,000 titik tersimpan; 1.26 s (7.94 batch/s); p50 1,241.37 ms, p95 1,245.46 ms |
| Race end-session vs ingestion | 10/10 ingestion dan 10/10 end HTTP 200; p95 masing-masing 128.59 ms dan 92.72 ms |
| History | Follow-up 30/30 HTTP 200; 3,000 row; 0.188 s; 159.81 req/s; p50 56.74 ms, p95 73.68 ms, max 80.62 ms |

Percobaan history pertama mengirim 50 halaman dari satu IP, sehingga 20 terkena rate limit API 30 request/menit per IP. Itu batas sampel harness, bukan kegagalan query. Follow-up menggunakan tepat 30 halaman dalam jendela batas dan semuanya sukses. Threshold yang diminta tercapai: steady ingestion p95 <300 ms, history p95 <500 ms; backfill p95 dicatat sebagai karakterisasi, bukan SLO task.

`EXPLAIN ANALYZE` history memakai `positions_latest_device_idx`, Index Scan, 101 row, execution 0.44 ms, planning 2.172 ms, 93 shared-buffer hits.

## Forecast storage

- Setelah seluruh skenario: 1,008,210 posisi; ukuran relasi total 519,913,472 byte, atau 515.68 byte/posisi termasuk indeks.
- Asumsi 10 posisi diterima per detik tanpa henti selama 365 hari: 315,360,000 posisi dan proyeksi relasi posisi 162,624,763,224 byte (151.46 GiB).
- Proyeksi menggunakan ukuran row sintetis dan indeks saat ini; tidak termasuk WAL, backup, replica, overhead vacuum, ruang bebas/headroom. Ini estimasi perencanaan, bukan kapasitas produksi terjamin.

## Verifikasi suite

Hasil readiness yang dijalankan untuk OPS-04:

- `cargo fmt --all --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo test --workspace` — 17 passed; 15 server DB tests dan 4 storage DB tests di-ignore secara default.
- `GPS_TEST_DATABASE_URL=... cargo test --workspace -- --ignored --test-threads=1` pada PostGIS disposable — 15 server integration + 4 storage/migration tests passed (auth, tenant/grant, device/token, heartbeat, realtime, session/concurrency, ingestion, history, retention/migration).
- `pnpm test` — 8 files, 31 tests passed; `pnpm typecheck`, `pnpm lint`, `pnpm contracts:check` passed.
- `pnpm build` — passed; Vite memberi warning chunk MapLibre 1,094.33 kB (296.04 kB gzip), lazy-loaded.
- `pnpm --filter @backend-tracker/portal test:e2e` — 1 Chromium E2E passed; E2E memakai API mocks, bukan server hidup.
- `python3 -m py_compile scripts/ops04_load.py` — passed.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — 11 passed; tracker validate/render passed setelah pembaruan task.

Catatan: kontrak generator/OpenAPI lulus, tetapi checkout ini memiliki file Git tidak tracked sehingga pemeriksaan drift berbasis `git diff` tidak bisa membandingkan hasil terhadap commit. Pengujian portal E2E memakai fixture/mock. Benchmark bersifat satu run pada host pengembangan dan tidak mewakili jaminan kapasitas/availability produksi.

Container disposable `gps-ops04-db` dan volumenya dihapus setelah evidence disalin; tidak ada container/volume lain yang dihapus. Baseline produk tidak diubah.
