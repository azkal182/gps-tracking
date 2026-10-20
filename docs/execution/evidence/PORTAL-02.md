# PORTAL-02 — Peta live dan status kontak

Tanggal: 2026-10-20  
Status: selesai.

## Implementasi

- Route `Peta live` memakai MapLibre GL JS 6.15.0 secara lazy. Style dapat diatur lewat `VITE_MAP_STYLE_URL`, atau memakai MapTiler Streets dengan `VITE_MAPTILER_KEY`; attribution control selalu terlihat. Contoh konfigurasi berada di `apps/portal/.env.example`.
- Peta target hanya memakai snapshot `/targets/{target_id}/location` untuk sesi aktif. Marker lama dibedakan secara visual; target tanpa sesi/posisi tidak memiliki marker. Tabel status menampilkan sesi, konektivitas, status/umur posisi dan waktu kontak/sampel secara terpisah dalam WIB.
- Portal mengambil snapshot REST setelah subscription berhasil dan setiap reconnect. Event posisi dicocokkan dengan target, device dan session aktif; posisi dimenangkan oleh urutan `recorded_at` lalu ID decimal. Snapshot lama tidak menimpa posisi/status yang lebih baru. Heartbeat hanya mengubah konektivitas/kontak, dan status `no_position` menghapus marker.
- Cache query dan WebSocket terikat organisasi. Switch tenant membatalkan/menghapus cache lama dan menutup socket sebelum berlangganan target tenant baru. Target yang ditolak endpoint posisi tidak ditampilkan atau disubscribe; tiket realtime 401/403 tidak di-retry berulang.
- Peta hanya mengirim viewport/style request kepada penyedia tile; posisi ditampilkan sebagai marker lokal. Jika style belum dikonfigurasi atau provider gagal, panel tabel tetap tersedia.

## File

- `apps/portal/src/features/live/live-map-page.tsx`
- `apps/portal/src/features/live/live-map.tsx`
- `apps/portal/src/features/live/live-state.ts`
- `apps/portal/src/features/live/use-live-targets.ts`
- `apps/portal/src/features/live/map-config.ts`
- `apps/portal/src/features/live/*.test.ts(x)`
- `apps/portal/src/App.tsx`, `apps/portal/src/components/portal-shell.tsx`, `apps/portal/src/index.css`
- `apps/portal/package.json`, `pnpm-lock.yaml`, `apps/portal/.env.example`

## Verifikasi

| Perintah | Hasil |
|---|---|
| `pnpm lint` | Lulus: ESLint dan Prettier. |
| `pnpm typecheck` | Lulus. |
| `pnpm test` | Lulus: 23 test di 5 file; termasuk alur halaman live, provider fallback, penolakan akses posisi, switch tenant/socket, heartbeat tanpa marker movement, event sesi lama dan snapshot reconnect lama. API/WS menggunakan fixture sintetis. |
| `pnpm build` | Lulus. MapLibre dimuat sebagai vendor chunk terpisah hanya ketika peta terkonfigurasi; bundler memberi warning ukuran chunk vendor 1,094.33 kB (296.04 kB gzip), di luar chunk halaman live 13.06 kB. |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | Lulus: 11 tests. |
| `python3 scripts/project_tracker.py validate` | Lulus setelah update backlog. |
| `python3 scripts/project_tracker.py render` | Lulus; view STATUS/BACKLOG dibuat dari backlog canonical. |

## Batas verifikasi dan berikutnya

E2E portal memakai React Testing Library/jsdom dengan API, WebSocket dan MapLibre sintetis; belum ada run terhadap backend HTTP hidup atau akun/provider MapTiler nyata. Key provider tetap harus diisi operator dengan pembatasan origin dan map-read. Status segar ditentukan event/backend contract; bila koneksi WS terputus portal mengambil snapshot ulang dan menampilkan status reconnect.

Berikutnya: PORTAL-03 — histori lokasi dengan filter waktu, pagination dan batas tampilan sesuai planning.
