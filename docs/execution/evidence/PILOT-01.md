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
