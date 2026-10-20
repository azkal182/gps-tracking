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

Dengan akses SSH staging yang diotorisasi pengguna, workflow [Actions run #3](https://github.com/azkal182/gps-tracking/actions/runs/39384598048) untuk commit `8e7e3f0994f417fb4364fcacbf68e37a0f05fecd` lulus quality, build/publish, dan deploy. Status service setelah deploy: API healthy, DB healthy, portal up, retention up tanpa healthcheck HTTP yang tidak sesuai. `https://staging.banza.web.id/healthz` mengembalikan `{"status":"ok"}` dan `/readyz` mengembalikan `{"status":"ready"}`. Worker retensi menjalankan purge dan melaporkan semua kategori 0. Aktivasi email dan login browser berhasil menurut pengguna.

Image staging saat ini: API `8e7e3f0994f417fb4364fcacbf68e37a0f05fecd`, digest `sha256:fa72b1580495f27a90a4c88563a669ebb1cc6e12af98f845f6818af5727435d3`; portal digest `sha256:68f2f7a1f06591a9ffe04fba5d019330e421e829afd0d92be9d9aa653b2982a0`. Candidate build sebelumnya `4faa2eda765d785c00c7d02c631b3d051dd790a1`: API digest `sha256:616a16b4ad47f5884fcc83f6849d7c1a21a5b4553b44a9da16b5ad14658c4073`; portal digest `sha256:afa0d1f0ba404a13da0ae831b9f9b2adef0349aacd1426967fab0200c3849b65`.

Uji kompatibilitas menjalankan API image sebelumnya terhadap schema staging saat ini di container sementara yang hanya bind ke loopback VPS. `/readyz` menjadi ready dan request `GET /v1/organization` dengan `X-Organization-Id` sintetis tanpa session ditolak HTTP 403. Request serupa pada API aktif juga ditolak 403. Container sementara dihapus setelah smoke. Ini membuktikan bahwa header tenant tanpa session tidak memberi akses; uji tenant A ke objek tenant B dengan session terautentikasi belum dilakukan.

Restore aktual dump `tracker-20261020T065642Z.dump` lulus di database `template0` pada container PostGIS sementara dengan `pg_restore --no-owner --no-acl --exit-on-error`, role capability sintetis, dan jaringan Docker `none`. Restore berisi 19 migrasi dan PostGIS 3.5.7; container dihapus setelah verifikasi. Uji ini tidak mengukur RPO/RTO maupun pemulihan dari salinan off-host.

Portal session yang tersedia menunjukkan belum ada organisasi aktif atau device, sehingga smoke map/history dengan titik sintetis dan lintas-organisasi terautentikasi belum dapat dilakukan tanpa menyiapkan data tenant. Checklist domain, SMTP, host, dan backup terverifikasi; konfigurasi tile staging masih perlu dicek bersama smoke map. PILOT-01 tetap blocked sampai verifikasi tenant dan map tersebut selesai serta evidence dicatat.
