# CONTRACT-01 — OpenAPI tracking lengkap

Tanggal: 19 Oktober 2026. Status: `done`.

## Implementasi

- Menambahkan `contracts/openapi.yaml` sebagai kontrak OpenAPI 3.1 untuk 19 operasi tracking REST: target, device, rotasi credential device, tracking session, posisi, dan heartbeat. Ticket/event WebSocket serta endpoint management umum tetap menjadi cakupan CONTRACT-02.
- Menetapkan mode autentikasi terpisah untuk portal cookie dan CSRF, application bearer, serta device bearer khusus ingestion. `X-Organization-Id` didokumentasikan sebagai konteks tenant yang tetap diverifikasi backend; resource tenant membawa `tenant_id`.
- Mendokumentasikan scope dan object access, request ID, success `Location`, idempotency key sesi (wajib, 1–128 karakter, replay 24 jam dengan reauthorization), rate limit, ukuran batch, pagination/cursor, rentang histori, dan error envelope/status.
- Schema menjaga pemisahan `tracking_status`, `connectivity_status`, dan `position_status`, memasukkan `observed_at`, serta merepresentasikan ID posisi sebagai string desimal. Contoh tidak berisi koordinat atau credential nyata.
- Menambahkan Redocly CLI versi tepat `2.45.0`, script `pnpm contracts:lint`, dan konfigurasi validasi contoh request/response terhadap schema. Kontrak OpenAPI ditautkan dari indeks dokumentasi.

## Berkas

- `contracts/openapi.yaml`, `redocly.yaml`
- `package.json`, `pnpm-lock.yaml`
- `docs/README.md`
- `docs/execution/backlog.json`, `docs/execution/WORKLOG.md`

## Verifikasi

| Perintah                                                                                     | Hasil                                                                                      |
| -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `pnpm install --no-frozen-lockfile`                                                          | PASS; Redocly dipasang dan lockfile diperbarui dengan versi tepat.                         |
| `pnpm install --frozen-lockfile`                                                             | PASS; 509 package terselesaikan dari cache, lockfile up-to-date.                           |
| `pnpm exec prettier --check contracts/openapi.yaml redocly.yaml package.json docs/README.md` | PASS; semua berkas cocok format.                                                           |
| `pnpm contracts:lint`                                                                        | PASS; OpenAPI valid, tanpa warning; aturan contoh media type, parameter, dan schema aktif. |
| `python3 scripts/project_tracker.py validate`                                                | PASS; 39 task, dependency, evidence path, dan coverage planning/skill valid.               |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'`                               | PASS; 11 test tracker.                                                                     |

## Kriteria dan batas

Acceptance CONTRACT-01 terpenuhi. Validasi membuktikan spesifikasi dan contoh yang dideklarasikan sesuai schema, bukan perilaku server. Belum ada endpoint tracking runtime untuk integration, authorization, malformed input, maupun concurrency tests; verifikasi tersebut dilakukan pada task implementasi terkait. Management auth/MFA/invitation/grants/application-token API dan skema WebSocket belum termasuk deliverable ini.

## Langkah berikutnya

Lanjutkan task dependency-ready `CONTRACT-02` — OpenAPI management dan schema WebSocket.
