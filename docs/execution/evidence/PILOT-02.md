# PILOT-02 — Pilot deployment terotorisasi

Tanggal: 2026-10-20
Status: **blocked — staging hidup; backup/WAL off-host dan tenant isolation smoke belum lengkap**

## Bukti yang dikumpulkan

| Pemeriksaan | Hasil |
|---|---|
| GitHub Actions run [#4](https://github.com/azkal182/gps-tracking/actions/runs/39385697463), commit `3364b94d7bcada3f29811f6ae19db8c772a4b963` | Quality, migration CI, build/publish dua image, dan deploy staging semuanya sukses. |
| SSH `docker compose ... ps --format json` | API healthy; DB healthy; portal running; retention running. Tidak membaca `.env`. Image tag yang berjalan adalah commit `3364b94d7bcada3f29811f6ae19db8c772a4b963`. |
| `curl -fsS https://staging.banza.web.id/healthz` | `{"status":"ok"}` |
| `curl -fsS https://staging.banza.web.id/readyz` | `{"status":"ready"}` |
| `curl -sSI https://staging.banza.web.id/` | HTTP/2 200. |
| Query DB staging `SELECT count(*) FROM _sqlx_migrations; SELECT max(version) FROM _sqlx_migrations;` | 19 migration tercatat; versi tertinggi `202610200003`. |
| `crontab -l | grep -F 'ops/staging/backup.sh'` untuk user deploy | Tidak menemukan jadwal backup. Lima dump lokal ada; yang terbaru saat pemeriksaan berukuran 206,787 byte. Backup aktual dan restore terisolasi dibuktikan pada PILOT-01. |
| `SHOW archive_mode; SHOW archive_command; SELECT pg_is_in_recovery();` | `off`, `(disabled)`, `f`; continuous WAL archiving tidak aktif. |
| Uji provider email | Pengguna telah mengonfirmasi menerima email aktivasi dan berhasil mengaktifkan akun; ini membuktikan alur SMTP staging berfungsi saat itu. |
| Uji isolasi tenant dengan sesi terautentikasi | Tidak dijalankan. Pemeriksaan lintas-organisasi tersebut secara eksplisit ditunda pengguna hingga uji coba manual; jangan dianggap lulus. |

## Batas dan unblock

PILOT-02 belum memenuhi acceptance backup/WAL monitoring. Backup yang ada berada lokal pada host yang sama, tidak ada jadwal backup harian terdeteksi, PostgreSQL tidak mengarsipkan WAL, dan alert untuk kegagalan/lag belum terbukti aktif. Runbook pgBackRest memerlukan repository terpisah, cipher key dan pemantauan; tidak ada konfigurasi repository off-host yang dapat diverifikasi. Tidak mengaktifkan konfigurasi backup baru dengan menyimpan lokasi GPS dan kunci enkripsi hanya pada host yang sama.

Smoke lintas-organisasi terautentikasi juga masih ditunda mengikuti arahan pengguna sebelumnya. Uji tanpa sesi yang menolak header tenant 403 dan uji kompatibilitas image lama tercatat pada [PILOT-01](./PILOT-01.md), tetapi keduanya bukan pengganti smoke terautentikasi.

Untuk melanjutkan: siapkan/otorisasi repository backup terenkripsi di luar VPS beserta mekanisme secret dan alert WAL/backup; setelah terpasang lakukan backup/WAL check; pengguna memberi tahu saat siap untuk uji manual lintas-organisasi atau mengarahkan agar pemeriksaan itu dibuka kembali.
