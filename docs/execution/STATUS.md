# Status eksekusi proyek

> Generated dari backlog.json; jangan edit langsung.

Tanggal data: 2026-10-20

**Siap deployment; pilot belum selesai.**

Implementasi MVP: **35/38 task done**. P00 dan POST tidak dihitung; ini bukan persentase waktu/effort.

| Fase | Done/total | Status |
|---|---:|---|
| P00 — Planning dan workflow agent | 2/2 | 2 done |
| P01 — Foundation proyek | 4/4 | 4 done |
| P02 — Kontrak API dan WS | 3/3 | 3 done |
| P03 — Database dan isolasi tenant | 3/3 | 3 done |
| P04 — Authentication dan recovery | 3/3 | 3 done |
| P05 — Organisasi, grants dan onboarding | 3/3 | 3 done |
| P06 — Target/device dan kredensial | 4/4 | 4 done |
| P07 — Session dan ingestion | 3/3 | 3 done |
| P08 — Heartbeat, query dan realtime | 3/3 | 3 done |
| P09 — Portal management dan peta | 3/3 | 3 done |
| P10 — Hardening dan readiness | 6/6 | 6 done |
| P11 — Packaging dan pilot MVP | 0/3 | 1 blocked, 2 todo |
| POST — Sesudah MVP | 0/3 | 3 deferred |

## Handoff

Sedang dikerjakan: tidak ada

Task siap: tidak ada

TENANT-01 menutup cross-tenant admin object checks dan memperbaiki runtime role dev. Portal browser kini menampilkan Histori tenant-scoped dan realtime Live tersambung.

Langkah berikut: Lanjutkan PILOT-01 sesuai environment gate; development browser sudah dapat dipakai untuk uji manual tenant-scoped.

Blocked PILOT-01: Staging menerima email aktivasi, tetapi smoke test menemukan portal tidak membaca token fragment (#token=...) yang dikirim backend. Perbaikan parser dan regression test lokal lulus; deployment portal hasil perbaikan dan aktivasi/MFA browser belum diverifikasi. Smoke tenant, restore backup, dan kompatibilitas image sebelumnya terhadap schema juga masih memerlukan evidence. — Push perbaikan parser token aktivasi, tunggu GitHub Actions deploy berhasil, lalu buka ulang tautan aktivasi yang masih berlaku dan verifikasi aktivasi/MFA. Lengkapi smoke tenant, tes restore backup serta kompatibilitas image sebelumnya terhadap schema; catat digest dan evidence aktual.

Batas verifikasi:

Detail: [BACKLOG](./BACKLOG.md), [workflow](./WORKFLOW.md), [worklog](./WORKLOG.md).
