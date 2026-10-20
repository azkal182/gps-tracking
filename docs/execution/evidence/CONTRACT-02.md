# CONTRACT-02 — OpenAPI management dan schema WS

Tanggal: 19 Oktober 2026. Status: `done`.

## Implementasi

- Memperluas `contracts/openapi.yaml` dengan 42 operasi auth/MFA/recovery, invitation acceptance, organisasi/platform, membership/grants, device credential metadata/revocation, application token, audit, realtime ticket, dan WebSocket upgrade. Digabung dengan 19 operasi CONTRACT-01 menjadi 61 operasi.
- Mendefinisikan request/response schema untuk login dengan hasil authenticated atau restricted MFA challenge, TOTP enrollment dan confirmation, recovery, password lifecycle, browser sessions, organisasi/membership, invitation, grant, credential metadata, integration token, audit, dan ticket.
- Memisahkan portal cookie + CSRF, auth challenge cookie, application bearer, WebSocket ticket, dan device bearer. Management API hanya memakai sesi portal; `POST /realtime/tickets` menerima sesi portal+CSRF atau application bearer `tracking:subscribe`; device bearer tidak diterima pada WS. Tenant context, role/scope, recent authentication/MFA, pagination, error responses, `X-Request-Id`, rate limit dan `Cache-Control: no-store` didokumentasikan.
- Menambahkan schema pesan client/server WebSocket, subscribe replacement, acknowledgment, safe error, dan enam event (`position.updated`, `session.started`, `session.ended`, `tracking.status_changed`, `device.connectivity_changed`, `position.status_changed`). Tiga status event mewajibkan `observed_at`. Access description membatasi target/device projection dan mensyaratkan pengecekan tenant/grant pada subscribe serta event delivery. Limit koneksi, subscription, outbound queue, best-effort delivery dan resync REST dijelaskan.
- Menambahkan contoh sintetis/redacted untuk resource, grants, audit, auth state, ticket, subscription dan event. Tidak ada password, MFA/recovery code, token atau koordinat nyata.
- Memperbarui deskripsi OpenAPI dan indeks docs. Ambiguitas principal saat mint ticket dicatat sebagai [EXEC-002](../DECISIONS.md): contoh API Contract mengizinkan application bearer untuk mint ticket, sementara Management API menetapkan cookie+CSRF bagi browser; kontrak mempertahankan kedua mode dengan scope/tenant binding.

## Berkas

- `contracts/openapi.yaml`, `redocly.yaml`
- `docs/README.md`, `docs/execution/DECISIONS.md`
- `docs/execution/backlog.json`, `docs/execution/WORKLOG.md`

## Verifikasi

| Perintah / inspeksi                                                                                         | Hasil                                                                                                                                                                                                 |
| ----------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm exec prettier --check contracts/openapi.yaml redocly.yaml docs/README.md docs/execution/DECISIONS.md` | PASS; semua berkas cocok format.                                                                                                                                                                      |
| `pnpm contracts:lint`                                                                                       | PASS; OpenAPI valid tanpa warning. Rule invalid media-type, parameter, dan schema examples tetap error.                                                                                               |
| Inspeksi/assertion YAML dengan Ruby standard library                                                        | PASS; 61 operasi total, 42 operationId management/realtime sesuai scope, request ID, binding tenant, mode auth, cap subscription 100, `observed_at`, dan larangan device principal pada WS diperiksa. |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'`                                              | PASS; 11 test tracker.                                                                                                                                                                                |
| `python3 scripts/project_tracker.py validate`                                                               | PASS; struktur, dependensi, evidence dan coverage skill/planning valid.                                                                                                                               |

Konfigurasi Redocly menonaktifkan `operation-2xx-response` karena handshake WebSocket berhasil dengan HTTP `101`; `no-unused-components` dinonaktifkan karena schema pesan dirujuk melalui extension `x-websocket-*` yang tidak dikenali sebagai `$ref` standar oleh linter. Validasi schema examples dan seluruh lint lain tetap aktif.

## Kriteria dan batas

Acceptance CONTRACT-02 terpenuhi. Lint membuktikan kualitas dokumen dan contoh terhadap schema; inspeksi menghitung cakupan route dan mengecek batas otorisasi yang tercatat. Endpoint auth/management serta runtime WebSocket belum diimplementasikan, sehingga tidak ada klaim bahwa MFA, atomic replay, tenant authorization, ticket single-use, revocation atau event filtering telah berjalan. Negative/concurrency/runtime tests mengikuti task implementasi auth dan realtime.

## Langkah berikutnya

Lanjutkan task dependency-ready `CONTRACT-03` — generate TypeScript client dan CI contract checks.
