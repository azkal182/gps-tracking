# FOUND-02 — Scaffold portal dan tooling frontend

Tanggal: 19 Oktober 2026. Status: `done`.

## Implementasi

- Root pnpm workspace mengelola `apps/portal`; versi package manager tercatat di `package.json`, dependency dikunci dalam `pnpm-lock.yaml`, dan generated output `node_modules`, `dist`, `coverage`, serta environment files diabaikan.
- Portal menggunakan React 19, Vite 8, TypeScript 6 dengan `strict: true`, Tailwind CSS 4 dan komponen shadcn/ui dari preset Nova/Base UI.
- Dependensi sesuai baseline dipasang: React Router (declarative routes), TanStack Query, React Hook Form, Zod, resolver, Vitest, React Testing Library, ESLint dan Prettier. Zustand tidak ditambahkan karena belum ada kebutuhan state lintas komponen.
- Shell responsif menyediakan route Overview, Devices, History dan 404, route loading dan render-error fallback yang dapat diakses.
- Isi portal menyatakan bahwa auth, organisasi, device, API, peta dan histori belum diimplementasikan; tidak ada token atau koordinat yang disimpan.
- Vite meneruskan semua `/v1` HTTP dan WebSocket ke `VITE_API_PROXY_TARGET` (default `http://127.0.0.1:8080`). Tidak ada SPA fallback untuk kegagalan API.
- README portal mendokumentasikan perintah dan proxy; `.env.example` hanya memuat target proxy nonsecret.

## Berkas utama

- `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `.gitignore`, `eslint.config.js`, `.prettierrc.json`
- `apps/portal/package.json`, `apps/portal/vite.config.ts`, `apps/portal/tsconfig*.json`, `apps/portal/components.json`
- `apps/portal/src/App.tsx`, `apps/portal/src/components/*`, `apps/portal/src/pages/*`, `apps/portal/src/test/setup.ts`, `apps/portal/src/App.test.tsx`
- `apps/portal/src/index.css`, `apps/portal/src/components/ui/button.tsx`, `apps/portal/src/lib/utils.ts`, `apps/portal/README.md`, `apps/portal/.env.example`

## Verifikasi

| Perintah / smoke check | Hasil |
|---|---|
| `pnpm install --frozen-lockfile` | PASS; workspace terpasang dari lockfile tanpa resolusi dependency baru |
| `pnpm typecheck` | PASS; `tsc -b --pretty` dengan TypeScript strict |
| `pnpm test` | PASS; 4 tes untuk route shell, 404, loading dan error boundary |
| `pnpm build` | PASS; Vite menghasilkan aset statis, overview sebagai lazy chunk |
| `pnpm lint` | PASS; ESLint dan Prettier |
| Proxy HTTP smoke: mock API `127.0.0.1:8080`, request `http://[::1]:5173/v1/positions` | PASS; respons 404 `application/json` dari mock API, bukan HTML SPA |
| Proxy WebSocket smoke: koneksi `ws://[::1]:5173/v1/ws/tracking` ke mock upgrade endpoint | PASS; handshake membuka koneksi |

## Batas dan langkah berikutnya

Acceptance FOUND-02 terpenuhi. Shell ini bukan autentikasi atau fitur management yang selesai. Integrasi API sesungguhnya mengikuti kontrak dan task portal berikutnya; map provider/key belum digunakan. Smoke proxy menggunakan mock lokal, bukan test terhadap deployment.
