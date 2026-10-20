# FOUND-03 — Environment lokal dan CI dasar

Tanggal: 19 Oktober 2026. Status: `done`.

## Implementasi

- `compose.yaml` menyediakan PostgreSQL 17 + PostGIS dan Mailpit versi `v1.31.6` untuk development lokal. Kedua image dipatok dengan tag dan sha256 digest. Port database, SMTP, dan UI Mailpit hanya bind ke `127.0.0.1`; password database wajib diisi dari environment dan tidak memiliki default.
- `.env.example` memberi nama database/user/port dengan password kosong, tanpa secret. `.env` sudah diabaikan Git. Panduan lokal mencakup generator password, start/wait, pemeriksaan PostGIS, endpoint SMTP/UI, penghentian, serta penghapusan volume disposable secara eksplisit.
- `.node-version` mematok Node `24.21.0`; `rust-toolchain.toml` mematok Rust `1.99.1` beserta `clippy`/`rustfmt`; pnpm `12.11.2` dan dependency ada di manifest/lockfile yang telah tersedia.
- `.github/workflows/ci.yml` menjalankan dependency install frozen, Rust format/clippy/test, portal lint/typecheck/test/build pada push dan pull request. Workflow memiliki permission minimum `contents: read`.
- Panduan operasional ditaruh di `docs/execution/development-environment.md` agar tidak diklasifikasikan sebagai dokumen planning oleh skill coverage tracker.

## Berkas

- `.env.example`, `.node-version`, `rust-toolchain.toml`, `compose.yaml`
- `.github/workflows/ci.yml`
- `docs/execution/development-environment.md`, `docs/README.md`
- `docs/execution/backlog.json`, `docs/execution/WORKLOG.md`

## Verifikasi

| Perintah / smoke check                                                                                                      | Hasil                                                         |
| --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `cargo fmt --all -- --check`                                                                                                | PASS                                                          |
| `cargo clippy --workspace --all-targets -- -D warnings`                                                                     | PASS                                                          |
| `cargo test --workspace`                                                                                                    | PASS; 2 tes server lulus, crate lain belum memiliki unit test |
| `pnpm install --frozen-lockfile`                                                                                            | PASS; lockfile up-to-date dan tidak ada resolusi baru         |
| `pnpm lint`                                                                                                                 | PASS; ESLint dan Prettier                                     |
| `pnpm typecheck`                                                                                                            | PASS; TypeScript project build/typecheck                      |
| `pnpm test`                                                                                                                 | PASS; 4 tes frontend lulus                                    |
| `pnpm build`                                                                                                                | PASS; Vite menghasilkan static assets                         |
| `pnpm exec prettier --check compose.yaml .github/workflows/ci.yml docs/execution/development-environment.md docs/README.md` | PASS                                                          |
| `docker compose ... config -q` dan `up -d --wait` dengan env/port sementara, project `backend-tracker-found03-validation`   | PASS; PostgreSQL dan Mailpit healthy                          |
| `CREATE EXTENSION IF NOT EXISTS postgis; SELECT PostGIS_Full_Version() IS NOT NULL;` di container DB uji                    | PASS; ekstensi tersedia (`t`)                                 |
| GET `http://127.0.0.1:18025/api/v1/info` dan cek `Version == v1.31.6`                                                       | PASS; HTTP 200 dan versi sesuai pin                           |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'`                                                              | PASS; 11 tracker tests                                        |

Compose diuji dengan password acak sintetis di file sementara mode `0600`; trap menghapus file, container, network dan volume project uji. Pengecekan Docker setelahnya tidak menemukan container atau volume milik project uji. Port uji berbeda dari default: `55439`, `11025`, `18025`. Tidak ada email keluar atau data produksi.

## Kriteria dan batas

Acceptance FOUND-03 terpenuhi. Workflow GitHub Actions belum dijalankan pada hosted runner; semua command quality yang dimuat di dalamnya dijalankan lokal. CI belum menjalankan migration/integration test karena migrasi database belum dibuat. Compose hanya untuk development; belum ada packaging atau deployment produksi.

## Langkah berikutnya

Lanjutkan task dependency-ready berikutnya: `CONTRACT-01` — OpenAPI tracking lengkap.
