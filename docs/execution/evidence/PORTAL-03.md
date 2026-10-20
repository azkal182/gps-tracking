# PORTAL-03 — Histori, filter dan audit portal

Tanggal: 2026-10-20  
Status: selesai

## Hasil

Halaman `/history` kini menyediakan pemilihan histori berdasarkan device atau tracking session, filter rentang waktu lokal dengan batas maksimum 31 hari, dan pengiriman batas RFC 3339 UTC ke API (`from` inklusif, `to` eksklusif). Zona waktu browser ditampilkan dan dipakai pada timestamp tabel. Device nonaktif tetap tersedia sebagai sumber histori sehingga data yang masih dalam retensi dapat dibaca oleh pengguna yang diotorisasi backend.

Tabel posisi dan audit memakai TanStack Table serta cursor opaque dari server. Tabel posisi mengambil 100 baris per halaman; peta memuat halaman 1.000 posisi hanya ketika diminta dan membatasi overlay pada 10.000 titik. Jika masih ada cursor setelah batas itu, UI menyebut data peta parsial dan meminta rentang dipersempit. Data posisi tidak ditulis ke localStorage/sessionStorage. Query dikunci dengan tenant aktif dan respons device, session, posisi serta audit diperiksa terhadap tenant sebelum ditampilkan. `403` ditampilkan sebagai akses ditolak yang aman. Bagian audit hanya diminta dan ditampilkan bagi admin organisasi.

MapLibre menggambar GeoJSON point/line lokal dan selalu menampilkan attribution; kegagalan konfigurasi/provider mempertahankan tabel histori. Route placeholder diganti oleh halaman fitur terpisah. Playwright Chromium ditambahkan untuk menguji journey browser dengan API fixture, termasuk rentang UTC, cursor, histori device nonaktif, session endpoint, audit, dan penyimpanan browser.

## Evidence dan verifikasi

- `pnpm --filter @backend-tracker/portal typecheck` — lulus.
- `pnpm --filter @backend-tracker/portal lint` — lulus tanpa warning.
- `pnpm --filter @backend-tracker/portal test` — lulus, 8 file / 31 test.
- `pnpm --filter @backend-tracker/portal build` — lulus. Vite tetap memberi warning ukuran vendor MapLibre 1,094.33 kB (296.04 kB gzip); chunk tersebut dimuat secara lazy.
- `PLAYWRIGHT_BROWSERS_PATH=/private/tmp/backend-tracker-playwright pnpm --filter @backend-tracker/portal exec playwright test --timeout=10000` — lulus, 1 E2E Chromium. Chromium dipasang sementara pada `/private/tmp/backend-tracker-playwright`.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — lulus, 11 test.
- `python3 scripts/project_tracker.py validate` — lulus setelah backlog diperbarui.
- `python3 scripts/project_tracker.py render` — dijalankan untuk memperbarui tampilan status/backlog.

## Batas verifikasi

Browser E2E menggunakan API fixture; tidak ada backend, database, atau tile provider live yang dijalankan pada task ini. Akses tenant/grant, rentang histori session `[started_at, ended_at)`, expiry satu tahun, dan audit event tetap ditegakkan backend; portal mengirim filter ke endpoint resmi dan hanya menampilkan response yang lolos pemeriksaan tenant. Integrasi runtime server/provider tidak diklaim telah diuji di sini.
