# Environment development lokal

Environment ini hanya untuk pengembangan lokal. PostgreSQL/PostGIS dan Mailpit
hanya bind ke loopback; Mailpit menampung email development dan tidak meneruskan
email ke penerima sungguhan. Volume database bertahan setelah container berhenti
dan hanya dihapus secara eksplisit.

## Menjalankan layanan

Prasyarat: Docker Desktop aktif dan perintah `docker` tersedia di terminal, Node.js `24.21.0`, pnpm `12.11.2`, dan Rust
toolchain `1.99.1` (Rust dapat memakai komponen `rustfmt` dan `clippy` dari file
`rust-toolchain.toml`). Versi Node dan Rust juga dipakai CI; dependency JavaScript
dan Rust dikunci masing-masing di `pnpm-lock.yaml` dan `Cargo.lock`.
Image PostgreSQL/PostGIS dan Mailpit dipatok dengan tag dan digest agar isi image
tetap sama sampai pin diperbarui secara sengaja.

Jika Docker Desktop baru dipasang dan terminal belum mengenali `docker`, buka
terminal baru agar perubahan PATH termuat.

Salin sample environment, lalu isi password lokal acak untuk database:

```sh
cp .env.example .env
python3 -c 'import secrets; print(secrets.token_urlsafe(32))'
```

Masukkan hasil generator pada `POSTGRES_PASSWORD` di `.env`. File `.env` tidak
masuk Git. Jangan gunakan password produksi atau membagikan nilai tersebut.

Jalankan layanan dan tunggu healthcheck database:

```sh
docker compose up -d --wait
docker compose ps
docker compose exec -T db psql -U tracker_dev -d tracker_dev \
  -c 'SELECT PostGIS_Full_Version();'
```

Endpoint lokal:

- PostgreSQL/PostGIS: `127.0.0.1:54329`, database/user `tracker_dev`.
- SMTP Mailpit: `127.0.0.1:1025`.
- UI Mailpit: <http://127.0.0.1:8025>.

Port dapat diubah lewat variabel pada `.env`. Jangan mengubah bind address dari
loopback pada environment ini.

## Seed untuk uji coba portal di browser

`devseed` hanya berjalan jika `APP_ENV=development` dan URL seed mengarah ke
`localhost` dengan nama database berakhiran `_dev` atau `_test`. Perintah ini
menerapkan migration lalu membuat fixture sintetis yang idempotent: dua
organisasi, satu admin lokal sebagai anggota admin kedua organisasi, lima
target/device, lima tracking session aktif, dan 120 posisi di sekitar Jakarta
dan Surabaya. Seed tidak mengirim email dan tidak mengeluarkan device token.

Isi `MFA_ENCRYPTION_KEY` dan `OUTBOX_ENCRYPTION_KEY` di `.env` dengan key
URL-safe Base64 32-byte yang stabil untuk pengembangan lokal. Pertahankan key
MFA tersebut di antara restart agar MFA yang sudah didaftarkan tetap bisa
diverifikasi. `devseed` membutuhkan password pilihan Anda di environment, bukan
argumen CLI, dan menyimpan hanya hash Argon2id. Dari terminal baru:

```sh
bash
set -a
source .env
set +a
if [ -z "${GPS_DEV_SEED_PASSWORD:-}" ]; then
  read -s -r -p 'Password akun seed (15-128 karakter): ' GPS_DEV_SEED_PASSWORD
  printf '\n'
  export GPS_DEV_SEED_PASSWORD
fi
LOCAL_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}?sslmode=disable"
APP_ENV=development GPS_DEV_SEED_DATABASE_URL="$LOCAL_DB_URL" \
  cargo run --locked -p server --bin devseed
unset GPS_DEV_SEED_PASSWORD
```

Email login adalah `demo.admin@local.test`; password berasal dari `GPS_DEV_SEED_PASSWORD` di `.env` atau nilai yang dimasukkan secara tersembunyi di prompt. Simpan `.env` secara privat (mode file 0600 dianjurkan). Seed ulang tidak mengubah data yang sudah ada.
Seed user wajib mengikuti login dan setup MFA normal pada login pertama.

Siapkan login runtime least-privilege sekali setelah migration dan seed tersedia.
Jangan jalankan HTTP server dengan `tracker_dev`: user itu pemilik database lokal
(dan dapat melewati RLS). Runtime login `gps_dev_auth` hanya mewarisi capability
auth; `gps_dev_app` hanya mewarisi capability tracking. Password acak minimal 32 karakter hex disimpan lokal di `.env`; buat masing-masing
dengan `openssl rand -hex 24` sebelum menjalankan script.

```sh
set -a
source .env
set +a
bash scripts/dev_db_roles.sh
```

Jalankan backend di terminal lain. Server membaca `DATABASE_URL` dan
`TRACKING_DATABASE_URL` runtime dari `.env`; URL owner tetap dipakai hanya oleh
seed/migration development.

```sh
set -a
source .env
set +a
cargo run --locked -p server --bin server
```

Di terminal ketiga, muat `.env` agar `VITE_*` diteruskan ke Vite, lalu jalankan
portal dan buka <http://localhost:5173>:

```sh
bash
set -a
source .env
set +a
pnpm --filter @backend-tracker/portal dev -- --host 127.0.0.1
```

Peta membutuhkan `VITE_MAPTILER_KEY` atau `VITE_MAP_STYLE_URL`; keduanya masih
kosong pada `.env` yang dibuat. Halaman target, device, session, dan histori
tetap bisa diuji tanpa key peta.

## Menghentikan dan menghapus data lokal

```sh
docker compose down
```

Perintah tersebut mempertahankan volume database. Untuk menghapus database lokal
disposable secara permanen, jalankan secara eksplisit:

```sh
docker compose down --volumes --remove-orphans
```

Tidak ada data produksi yang boleh disimpan di volume ini.

## Quality checks lokal

Gunakan perintah CI yang sama dari root repository:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

Workflow GitHub Actions menjalankan seluruh pemeriksaan tersebut pada push dan
pull request. Ia belum menjalankan migration/integration test database karena
migration dan fitur storage belum menjadi bagian foundation ini.
