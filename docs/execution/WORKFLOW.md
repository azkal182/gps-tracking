# Workflow agentic proyek

Dokumen execution bersifat hidup, terpisah dari baseline planning FINAL. Skill hanya tersedia dalam repository ini melalui `.agents/skills`. Semua dokumen baseline dipetakan dalam skill `gps-plan`; skill domain membaca sumber asli, bukan salinan spesifikasi.

## Berkas dan otoritas

- `backlog.json`: status task, dependensi, acceptance, references, evidence dan handoff; source of truth.
- `STATUS.md`: ringkasan dan next-ready, generated oleh tracker.
- `BACKLOG.md`: semua fase/tugas dengan acceptance, generated oleh tracker.
- `WORKLOG.md`: catatan perubahan/progres kronologis.
- `DECISIONS.md`: perubahan teknis yang berdampak pada baseline, dengan alasan dan dokumen yang diselaraskan.
- `evidence/<ID>.md`: bukti implementasi/check, tidak berisi secret/data lokasi nyata.

## Status task

`todo`: belum dimulai; readiness diturunkan dari dependencies, bukan status manual.
`in_progress`: sedang dikerjakan, owner dan started_at wajib; task ini diprioritaskan saat resume.
`blocked`: ada hambatan nyata, blocked_reason dan unblock_action wajib; dependency belum selesai saja tidak cukup.
`done`: seluruh acceptance dipenuhi, deps done, evidence nonempty dan completed_at wajib.
`deferred`: hanya untuk scope post-MVP; tidak boleh dipakai menghilangkan P0 dari denominator.

Tugas roadmap boleh dipecah menjadi child task ketika terlalu besar, dengan ID baru stabil dan dependencies yang tetap menunjuk gate yang benar. Jangan menghapus tugas wajib atau menyatakan selesai hanya untuk meningkatkan progress. Jumlah done adalah hitungan task, bukan persentase effort/waktu.

## Fase sampai MVP

P00 = planning/agent tooling, bukan kemajuan implementasi produk. P01–P10 = foundation sampai aplikasi teruji dan kesiapan rilis. P11 = packaging dan pilot deployment, membutuhkan environment/otorisasi saat waktunya. MVP selesai hanya bila semua task wajib P01–P11 done, termasuk bukti release/restore. Jika P01–P10 selesai tanpa P11, sebut “siap deployment”, bukan “pilot live”. POST tidak termasuk MVP.

## Memakai skill dan tracker

Contoh permintaan:

- `Gunakan $gps-backlog untuk lanjutkan task MVP berikutnya yang siap, sesuai planning.`
- `Gunakan $gps-auth untuk mengerjakan AUTH-02 dan catat bukti verifikasinya.`
- `Gunakan $gps-plan untuk memeriksa dampak perubahan aturan session.`
- `Gunakan $gps-release untuk audit readiness MVP; jangan deploy.`

Jalankan dari root repo:

```sh
python3 scripts/project_tracker.py status
python3 scripts/project_tracker.py next
python3 scripts/project_tracker.py validate
python3 scripts/project_tracker.py render
```

Edit JSON secara eksplisit, jangan edit generated views. Validator memeriksa struktur/dependensi/evidence/coverage docs-skills; ia tidak dapat memastikan pengujian yang ditulis benar-benar dijalankan. Agent wajib memeriksa bukti dan code, dan reviewer memeriksa acceptance. `render` gagal jika data invalid.

## Format evidence

Untuk task selesai catat: scope/requirement IDs, file aktual, perubahan perilaku, tabel command/outcome/date atau hasil inspeksi yang relevan, kriteria acceptance yang dipenuhi, keterbatasan, next action. Gunakan tanggal aktual; jangan menyalin “passed” dari task lain. Evidence untuk blocked mencatat command/error atau input yang dibutuhkan, tanpa menyimpan credentials.

## Discovery lokal

Codex memindai `.agents/skills` repository. Skill baru dapat dideteksi otomatis; jika tidak muncul, mulai ulang Codex/chat dan cek picker. AGENTS.md juga menunjuk file secara langsung sehingga agent dapat membacanya tanpa instalasi global. Kehadiran file dan validasi tidak membuktikan bahwa picker pada sesi aktif sudah me-refresh.

Sumber: [OpenAI — local skills](https://learn.chatgpt.com/docs/build-skills), [AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md).
