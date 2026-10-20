# Deploy staging ke VPS dengan GitHub Actions

Panduan ini menyiapkan **staging**, bukan production. Setelah pull request ke
`main` lolos workflow `CI`, GitHub Actions membangun dua image, menerbitkannya ke
GitHub Container Registry (GHCR) dengan tag commit SHA, lalu mengirim konfigurasi
dan menjalankan deployment di VPS. Push ke branch lain atau pull request hanya
menjalankan CI; tidak mengubah VPS.

Deployment memakai satu domain dan Caddy untuk HTTPS serta routing. PostgreSQL +
PostGIS hanya berada di jaringan Docker privat. Backend berjalan sebagai user
non-root. Secret aplikasi dan database tinggal di file `.env` VPS; GitHub hanya
memegang kredensial deployment dan pembaca image. Posisi GPS bersifat sensitif:
gunakan data sintetis di staging.

## Yang disiapkan

- Repository ini sudah ada di GitHub, branch deployment bernama `main`, Actions
  aktif, dan akun memiliki akses ke GitHub Container Registry.
- Satu VPS Ubuntu 24.04 LTS amd64 dengan Docker Engine dan plugin Compose. Pilih
  CPU, RAM, disk dan backup sesuai volume uji; panduan ini tidak mengasumsikan
  kapasitas production.
- Subdomain staging, misalnya `staging.example.com`, dengan record DNS A ke IP
  VPS. Jika record AAAA ada, VPS juga harus bisa menerima IPv6.
- Akses console VPS dari penyedia, SSH key, dan relay SMTP staging dengan TLS.
- Opsional: MapTiler key yang dibatasi untuk domain staging. Kunci `VITE_*`
  dimasukkan ke bundle browser, jadi kunci itu memang dapat dilihat pengguna;
  batasi domain dan haknya. Jangan gunakan untuk secret server.

Docker dan GitHub dapat mengubah instruksi instalasi/fitur. Ikuti panduan resmi
terbaru untuk [install Docker Engine di Ubuntu](https://docs.docker.com/engine/install/ubuntu/),
[publikasi image ke GHCR](https://docs.github.com/en/actions/tutorials/publish-packages/publish-docker-images),
dan [GitHub deployment environments](https://docs.github.com/en/actions/concepts/workflows-and-actions/deployment-environments).

## 1. Siapkan VPS

1. Buat VPS Ubuntu dan catat IP publik. Dari panel firewall penyedia, izinkan
   inbound TCP 22 (SSH), 80 (HTTP untuk validasi domain/TLS), dan 443 (HTTPS).
   UDP 443 opsional untuk HTTP/3. Batasi SSH ke IP Anda bila penyedia mendukung.
2. Buat DNS A untuk subdomain staging yang menunjuk ke IP VPS. Tunggu sampai
   nama itu mengarah ke VPS sebelum Caddy meminta sertifikat HTTPS.
3. Pasang Docker Engine dan plugin Compose dari repositori resmi Docker. Untuk
   Ubuntu 24.04 amd64, langkah resminya saat dokumen ini dibuat:

   ```sh
   sudo apt update
   sudo apt install ca-certificates curl
   sudo install -m 0755 -d /etc/apt/keyrings
   sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
   sudo chmod a+r /etc/apt/keyrings/docker.asc
   sudo tee /etc/apt/sources.list.d/docker.sources <<EOF
   Types: deb
   URIs: https://download.docker.com/linux/ubuntu
   Suites: noble
   Components: stable
   Architectures: amd64
   Signed-By: /etc/apt/keyrings/docker.asc
   EOF
   sudo apt update
   sudo apt install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
   sudo systemctl status docker
   sudo docker run hello-world
   ```

   Docker mempublikasikan langkah terbaru di panduan Ubuntu resminya. Aturan
   firewall Docker dapat melewati konfigurasi UFW; utamakan firewall jaringan
   dari panel VPS dan jangan publikasikan port selain SSH/HTTP/HTTPS. Verifikasi:

   ```sh
   docker --version
   docker compose version
   ```

4. Buat user khusus `deploy` untuk Actions. Tambahkan user ke grup `docker` agar
   dapat menjalankan Compose, lalu siapkan direktori deployment:

   ```sh
   sudo adduser --disabled-password --gecos '' deploy
   sudo usermod -aG docker deploy
   sudo install -d -o deploy -g deploy -m 700 /opt/gps-tracker
   ```

   Akses grup `docker` setara akses root pada host. Batasi private SSH key user
   ini, jangan pakai untuk login manusia sehari-hari, dan jangan beri key ke job
   selain workflow staging.

5. Buat SSH key khusus deployment di komputer Anda, tanpa passphrase agar job
   Actions non-interaktif dapat menggunakannya. Pasang bagian public key sebagai
   `/home/deploy/.ssh/authorized_keys` di VPS; lindungi private key untuk
   dimasukkan sebagai GitHub Environment secret.
6. Catat host key VPS dari console tepercaya. Simpan baris `known_hosts` yang
   sudah diverifikasi; jangan mempercayai output `ssh-keyscan` tanpa membandingkan
   fingerprint-nya dengan console/panel provider.

## 2. Buat konfigurasi staging di VPS

Workflow akan mengirim folder `ops/staging` ke `/opt/gps-tracker`, tetapi tidak
menimpa file `.env` milik VPS. Buat konfigurasi pertama kali:

```sh
sudo -iu deploy
cd /opt/gps-tracker
cp ops/staging/.env.example .env
chmod 600 .env
```

Edit `.env` dan ganti seluruh nilai `REPLACE_...`:

- `GHCR_OWNER`: nama user/organisasi GitHub dalam huruf kecil.
- `STAGING_DOMAIN`: subdomain staging yang DNS-nya sudah diarahkan ke VPS.
- `POSTGRES_PASSWORD` dan lima `TRACKER_*_DB_PASSWORD`: masing-masing nilai
  acak berbeda. Buat nilainya dengan `openssl rand -hex 32`.
- `MFA_ENCRYPTION_KEY` dan `OUTBOX_ENCRYPTION_KEY`: dua kunci berbeda. Buat
  masing-masing dengan `openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n'`.
  Hasil ini berformat URL-safe Base64 yang diharapkan server.
- `SMTP_*`: host, port, TLS, alamat pengirim dan kredensial relay staging.
  Untuk port 587 pakai `SMTP_TLS=starttls`; username/password wajib untuk relay
  non-lokal. Kutip nilai jika berisi karakter khusus agar aman untuk format
  `.env` dan shell.
- `POSITION_RETENTION_DAYS`: default staging 365 hari. Nilai ini dibaca API dan
  worker purge; perubahan berlaku setelah service dibuat ulang pada deployment.
- `STAGING_BACKUP_DIR`: folder backup di VPS dengan ruang yang cukup. Backup ini
  lokal; ia tidak melindungi dari kehilangan seluruh VPS.

Simpan `.env` dengan permission `600`, jangan commit atau kirim ke GitHub. Jangan
mengganti kunci enkripsi setelah ada data MFA/outbox terenkripsi kecuali mengikuti
prosedur rotasi kunci. Contoh template memiliki `IMAGE_TAG` dummy; deployment
Actions menggantinya dengan SHA commit.

## 3. Buat GitHub Environment dan secrets

Di repository GitHub, buka **Settings → Environments → New environment**, beri
nama tepat `staging`, lalu batasi deployment branch ke `main`. Karena dipakai
sendiri, approval manual opsional; bila diaktifkan, publish image akan selesai
lebih dulu dan deploy menunggu approval environment.

Tambahkan secrets pada environment `staging`:

| Secret                | Isi                                                                     |
| --------------------- | ----------------------------------------------------------------------- |
| `STAGING_HOST`        | IP atau nama host VPS                                                   |
| `STAGING_USER`        | `deploy`                                                                |
| `STAGING_SSH_KEY`     | private key khusus Actions, termasuk baris BEGIN/END                    |
| `STAGING_KNOWN_HOSTS` | baris host key VPS yang sudah diverifikasi                              |
| `GHCR_USERNAME`       | user/organisasi yang memiliki token GHCR                                |
| `GHCR_READ_TOKEN`     | GitHub PAT classic khusus `read:packages`; hanya dipakai VPS untuk pull |

Tambahkan environment variables (bukan secrets) jika peta akan dipakai:

| Variable                | Isi                                                           |
| ----------------------- | ------------------------------------------------------------- |
| `STAGING_DOMAIN`        | domain sama dengan `.env` VPS; dipakai sebagai deployment URL |
| `STAGING_MAPTILER_KEY`  | kunci browser yang dibatasi ke domain staging, opsional       |
| `STAGING_MAP_STYLE_URL` | style URL lengkap, opsional bila memakai key MapTiler         |

Image dipublikasikan memakai `GITHUB_TOKEN` job dengan izin `packages: write`.
VPS memakai PAT terpisah yang hanya memiliki `read:packages`; jangan gunakan PAT
ini di repository source atau beri izin tulis. Pastikan package GHCR memberi
token tersebut akses pull. GitHub environment membatasi akses secrets ke job
deploy yang merujuk environment itu.

## 4. Hubungkan dan jalankan CI/CD

Workflow sudah berada di `.github/workflows/ci.yml`:

1. Pull request menjalankan audit dependency, validasi kontrak, format/lint,
   test backend dan portal, migration test serta build portal.
2. Hanya push ke `main` yang lolos `quality` membangun dan mem-publish image API
   dan portal dengan tag SHA commit.
3. Job deploy menyalin konfigurasi staging melalui SSH, login ke GHCR melalui
   stdin, menjalankan backup lokal, migration, provision login DB runtime,
   memperbarui service, lalu memeriksa `https://<STAGING_DOMAIN>/readyz`.

Commit dan push file project ke GitHub, lalu buka tab **Actions**. Setelah
Environment dan secrets siap, jalankan deployment dengan merge/push ke `main`.
Jangan menaruh SSH private key, PAT, `.env`, password DB, SMTP password, MFA key,
atau OUTBOX key di workflow, GitHub variables, issue, maupun log.

Push pertama menjalankan provisioning database otomatis:

- PostgreSQL dibuat di volume persisten dan tidak membuka port database ke host.
- Sebelum setiap migration, `pg_dump` membuat backup terverifikasi lokal di
  `STAGING_BACKUP_DIR` dan menghapus dump lokal yang lebih tua dari 14 hari.
- Migrasi SQLx memakai koneksi admin DB hanya pada one-shot migration container.
- Setelah migrasi, login `gps_http_*` dibuat/dirotasi dan hanya mewarisi role
  capability terbatas (`auth`, `app`, `platform`, `mailer`, `retention`). API
  tidak memakai login admin.
- Caddy memperoleh TLS otomatis dari Let's Encrypt dan meneruskan `/v1` serta
  WebSocket ke API. Path selain API/health dilayani sebagai portal SPA.
- Worker retensi berjalan setiap jam. Jika gagal, container keluar dan
  `restart: unless-stopped` mencoba kembali. Periksa log/alert sendiri; staging
  template belum mengirim notifikasi kegagalan otomatis.

## 5. Periksa hasil staging

```sh
curl -fsS https://staging.example.com/healthz
curl -fsS https://staging.example.com/readyz
cd /opt/gps-tracker
docker compose --env-file .env -f ops/staging/compose.yaml ps
docker compose --env-file .env -f ops/staging/compose.yaml logs --tail=100 api portal retention
```

Buka domain portal, aktifkan akun admin pertama lewat email aktivasi staging,
lalu uji login/MFA, pembuatan organisasi/device, target/session, histori, live
dan logout. Buat data sintetis; jangan menghubungkan device nyata sebelum backup,
restore, alerting, email dan akses jaringan staging benar-benar diverifikasi.

Jika perlu membuat akun platform admin pertama secara manual, `authctl` hanya
membuat akun pending dan mengirim aktivasi; ia tidak membuat password siap pakai:

```sh
docker compose --env-file .env -f ops/staging/compose.yaml exec api \
  authctl bootstrap-admin \
  --email 'admin@example.com' \
  --display-name 'Staging Admin' \
  --reason 'initial staging setup' \
  --case-id 'STAGING-BOOTSTRAP-001' \
  --operator 'VPS owner'
```

Jalankan hanya sekali; bootstrap ditolak jika platform admin sudah ada. Pastikan
alamat email tersebut milik Anda dan SMTP staging sudah berhasil mengirim.

## 6. Backup, perubahan dan rollback

Backup satu kali:

```sh
cd /opt/gps-tracker
./ops/staging/backup.sh
```

Untuk staging, pasang cron harian sebagai `deploy` (contoh pukul 03:00 UTC):

```sh
crontab -e
```

Tambahkan baris berikut pada editor yang terbuka:

```cron
0 3 * * * cd /opt/gps-tracker && ./ops/staging/backup.sh >> /home/deploy/staging-backup.log 2>&1
```

Atur rotasi `/home/deploy/staging-backup.log` melalui logrotate. Uji restore dump
secara berkala pada DB terisolasi;
backup belum layak disebut berhasil dipulihkan hanya karena file tercipta.
Simpan salinan terenkripsi di storage di luar VPS sebelum memakai staging dengan
data riil. Runbook production pgBackRest dan PITR ada di
[`ops/backup/restore-runbook.md`](../ops/backup/restore-runbook.md).

Workflow deployment menyimpan image per commit SHA. Untuk rollback aplikasi ke
SHA sebelumnya yang sudah diuji kompatibilitasnya dengan schema sekarang:

```sh
cd /opt/gps-tracker
IMAGE_TAG=<40-character-previous-commit-sha> ./ops/staging/rollback.sh
```

Script rollback hanya mengganti image API/portal/retention; **tidak** membatalkan
migration dan tidak merestore database. Jangan rollback image ke versi yang tidak
kompatibel dengan schema saat ini. Jangan menjalankan `docker compose down -v`;
opsi `-v` menghapus volume DB dan sertifikat Caddy.

## Troubleshooting singkat

- Workflow gagal konek SSH: cek IP/port, user, public key `authorized_keys`,
  `STAGING_KNOWN_HOSTS` dan firewall.
- `docker login`/`pull` gagal: cek package access `read:packages`, owner lowercase
  dan nama package GHCR.
- Caddy tidak mendapat sertifikat: pastikan DNS sudah propagasi dan port 80/443
  terbuka dari internet; lihat `docker compose logs portal`.
- API tidak healthy: lihat log `api`, `migrate`, dan `db`; pastikan runtime role
  password di `.env` tidak diganti berbeda dari yang di provision.
- SMTP gagal: cek TLS mode, port, pengirim yang diizinkan provider dan log API.
- Data hilang: jangan hapus Docker volume; hentikan perubahan, ambil snapshot
  kondisi saat ini dan gunakan restore runbook dalam DB terisolasi.
