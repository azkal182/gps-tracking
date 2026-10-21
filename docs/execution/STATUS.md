# Status eksekusi proyek

> Generated dari backlog.json; jangan edit langsung.

Tanggal data: 2026-10-21

**Siap deployment; pilot belum selesai.**

Implementasi MVP: **38/40 task done**. P00 dan POST tidak dihitung; ini bukan persentase waktu/effort.

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
| P09 — Portal management dan peta | 5/5 | 5 done |
| P10 — Hardening dan readiness | 6/6 | 6 done |
| P11 — Packaging dan pilot MVP | 1/3 | 1 blocked, 1 done, 1 todo |
| POST — Sesudah MVP | 0/3 | 3 deferred |

## Handoff

Sedang dikerjakan: tidak ada

Task siap: tidak ada

PORTAL-04 dan PORTAL-05 selesai: portal kini memiliki manajemen organisasi/anggota/grants serta detail dan lifecycle target/device dengan pencarian, cursor, konfirmasi dan credential metadata.

Langkah berikut: Lanjutkan dengan task MVP dependency-ready berikutnya; PILOT-02 tetap tercatat blocked sesuai arahan untuk menunda uji tenant manual.

Blocked PILOT-02: Staging deployment, migration, health/readiness, portal, and email activation smoke passed. Acceptance still lacks active off-host backup/WAL archive monitoring and authenticated cross-tenant smoke; the latter remains deferred per the user’s explicit prior direction. Staging currently has only local dumps, no backup cron match for deploy, archive_mode=off, and archive_command disabled. — Provision and authorize encrypted backup repository outside the VPS plus secret delivery and backup/WAL alerting; configure and verify pgBackRest backup/archive checks. When the user is ready, run the previously deferred authenticated cross-tenant smoke with synthetic tenant data, or receive explicit direction to keep that check deferred for this task.

Batas verifikasi:

Detail: [BACKLOG](./BACKLOG.md), [workflow](./WORKFLOW.md), [worklog](./WORKLOG.md).
