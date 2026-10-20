# PILOT-01 — Image dan deployment preflight

Tanggal: 2026-10-20
Status: **blocked — packaging dan panduan siap; deployment staging belum tersedia**

## Perubahan

- Menambahkan multi-stage `Dockerfile.server` (Rust release binary, runtime Debian
  slim, user UID/GID 10001 non-root) dan `Dockerfile.portal` (Vite build, Caddy
  static/reverse proxy/TLS), plus `.dockerignore`.
- Menambahkan migrator one-shot berbasis SQLx dan menggunakannya sebelum API
  berjalan. Migration login memakai database admin terpisah dari login runtime.
- Menambahkan Compose staging: PostgreSQL/PostGIS privat dan persistent, API,
  portal/Caddy, retensi per jam, health check, serta login runtime least-
  privilege. Tidak memublikasikan port database.
- Menambahkan provisioning role, backup `pg_dump` sebelum migration, deployment
  dan rollback application-image script, template `.env` tanpa secret, serta
  [panduan staging VPS dan GitHub Actions](../../staging-vps-github-cicd.md).
- Workflow CI menunggu quality job; push `main` lalu publish image API/portal ke
  GHCR berdasarkan commit SHA dan deploy melalui SSH dengan verifikasi host key.
  Branch lain/pull request tidak men-deploy.

## Verifikasi yang dijalankan

| Perintah                                                                                                                                                                         | Hasil                                                                                                 |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `cargo fmt --all -- --check`                                                                                                                                                     | Lulus                                                                                                 |
| `cargo check --workspace --all-targets`                                                                                                                                          | Lulus                                                                                                 |
| `cargo clippy --workspace --all-targets -- -D warnings`                                                                                                                          | Lulus                                                                                                 |
| `cargo test --workspace`                                                                                                                                                         | 20 passed, 0 failed; 19 database-dependent tests ignored karena `GPS_TEST_DATABASE_URL` tidak disetel |
| `pnpm lint`                                                                                                                                                                      | Lulus                                                                                                 |
| `pnpm typecheck`                                                                                                                                                                 | Lulus                                                                                                 |
| `pnpm test`                                                                                                                                                                      | 8 test files, 31 tests passed                                                                         |
| `pnpm build`                                                                                                                                                                     | Lulus; Vite mencetak peringatan chunk MapLibre >500 KiB                                               |
| `bash -n ops/staging/deploy.sh ops/staging/backup.sh ops/staging/provision-roles.sh ops/staging/rollback.sh`                                                                     | Lulus                                                                                                 |
| `pnpm exec prettier --write .github/workflows/ci.yml ops/staging/compose.yaml docs/staging-vps-github-cicd.md`                                                                   | Berhasil mem-parse/memformat workflow YAML, Compose YAML dan panduan                                  |
| `docker compose --env-file ops/staging/.env.example -f ops/staging/compose.yaml config --quiet`                                                                                  | Lulus dengan akses Docker daemon tereskalasi; template config tervalidasi                             |
| `docker build --check --file Dockerfile.server .`                                                                                                                                | Lulus; tidak membangun image                                                                          |
| `docker build --file Dockerfile.portal --tag backend-tracker-portal:preflight .`                                                                                                 | Image portal berhasil dibangun lokal; map key adalah konfigurasi browser publik                       |
| `docker run --rm -e STAGING_DOMAIN=staging.example.com -v "$PWD/ops/staging/Caddyfile:/etc/caddy/Caddyfile:ro" caddy:2.10.2-alpine caddy validate --config /etc/caddy/Caddyfile` | Lulus: `Valid configuration`                                                                          |

## Batas dan unblock

Image API server belum dibangun penuh lokal karena host hanya menyisakan sekitar
4 GiB setelah build portal; Dockerfile server lolos static check, dan workflow
GitHub akan membangun image penuh setelah push. Deployment/health/negative tenant
smoke dan image compatibility test belum berjalan. VPS, domain/DNS, relay SMTP,
serta GitHub `staging` environment dan secrets belum disediakan/dikonfigurasi.
Ini bukan bukti deploy gagal; staging belum tersedia untuk diverifikasi.

Unblock dengan menyiapkan VPS Ubuntu amd64, domain, SMTP staging, GHCR pull token,
SSH host key/key, dan Environment/secrets GitHub sesuai panduan. Setelah workflow mendorong image,
jalankan smoke test synthetic dan uji image lama terhadap schema hasil migration;
PILOT-01 tetap belum done sampai evidence itu tersedia. Jangan mengarahkan lokasi
nyata ke staging sebelum backup/restore dan alerting dipastikan.

## Temuan smoke aktivasi staging (2026-10-20)

Pengguna menerima pesan aktivasi, tetapi halaman menampilkan `Token aktivasi tidak ditemukan` dan tidak mengirim request API. Backend sengaja meletakkan token pada URL fragment `#token=...`; portal hanya membaca query string sehingga token tidak terbaca. Parser portal kini membaca token query maupun fragment, lalu menghapus token dari address bar sebelum digunakan. Regression test memastikan token fragment dikirim hanya saat submit dan hash dibersihkan. Verifikasi lokal: Prettier, portal lint, typecheck, dan Vitest 8 file / 32 test lulus. Deployment perbaikan dan aktivasi browser staging masih menunggu.

## Verifikasi staging langsung (2026-10-20)

Dengan akses SSH staging yang diotorisasi pengguna, `docker compose ps` menunjukkan API dan DB healthy; `https://staging.banza.web.id/healthz` mengembalikan `{"status":"ok"}` dan `/readyz` mengembalikan `{"status":"ready"}`. Log retention menunjukkan purge berhasil (semua kategori 0), tetapi status container `unhealthy` karena image API membawa healthcheck HTTP yang tidak sesuai untuk worker tanpa listener HTTP. Compose kini menonaktifkan inherited healthcheck untuk service retention; perlu deploy ulang untuk memverifikasi status container baru.

Smoke autentikasi email/aktivasi dan login browser berhasil menurut pengguna setelah image portal `2c4db4b21836903c695550e406084622aa584544` di-deploy. Image API build itu memiliki digest `sha256:048243130954460aa125668b8eb68132fcc7a2d1b4931a7d9feeeff4e0dda7c9`; portal `sha256:512b46a7d236350099c6487140b43655377d4fc6ed549dd8c3570f351c3ccbd8`.

Uji kompatibilitas menjalankan API image build sebelumnya `4faa2eda765d785c00c7d02c631b3d051dd790a1` terhadap schema staging saat ini di container sementara yang hanya bind ke loopback VPS. `/readyz` menjadi ready dan request `GET /v1/organization` dengan `X-Organization-Id` sintetis tanpa session ditolak HTTP 403. Digest API sebelumnya `sha256:616a16b4ad47f5884fcc83f6849d7c1a21a5b4553b44a9da16b5ad14658c4073`; portal sebelumnya `sha256:afa0d1f0ba404a13da0ae831b9f9b2adef0349aacd1426967fab0200c3849b65`. Container sementara dihapus setelah smoke. Ini membuktikan penolakan tenant header tanpa session; uji akses lintas-organisasi memakai session aktif belum dilakukan.

Restore aktual dump `tracker-20261020T065642Z.dump` lulus di database `template0` pada container PostGIS sementara dengan `--no-owner --no-acl --exit-on-error`, role capability sintetis, dan jaringan Docker `none`. Restore berisi 19 migrasi dan PostGIS 3.5.7; container dihapus setelah verifikasi. Uji ini tidak mengukur RPO/RTO maupun pemulihan dari salinan off-host.

Batas tersisa: deploy healthcheck Compose terbaru dan cek status retention; uji negatif dengan session terautentikasi lintas dua organisasi; kompatibilitas portal sebelumnya belum diuji di browser. Jangan tandai PILOT-01 selesai sebelum cakupan tersisa ini direkam.
