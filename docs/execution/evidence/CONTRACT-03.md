# CONTRACT-03 — Generate client dan CI contract checks

Tanggal: 19 Oktober 2026. Status: `done`.

## Implementasi

- Mengunci `openapi-typescript@7.13.0` pada root devDependencies dan `openapi-fetch@0.17.0` pada portal; lockfile pnpm diperbarui.
- Membuat `apps/portal/src/api/generated.ts` dari `contracts/openapi.yaml`, dan script `contracts:generate` menghasilkan ulang lalu memformat file secara deterministik. File generated tidak diubah manual.
- Menambahkan `apps/portal/src/api/client.ts` sebagai typed client portal. Client menggunakan same-origin `/v1` secara default, cookie credentials, dan callback konteks dinamis untuk `X-Organization-Id`/`X-CSRF-Token` pada mutasi. Header tenant hanya konteks request; otorisasi tetap tanggung jawab backend. Credential tidak disimpan di state client.
- Menambahkan `contracts:check` yang melint kontrak/contoh, menjalankan generator, lalu menggagalkan CI jika generated client berbeda. Workflow CI menjalankan pemeriksaan ini setelah frozen install.
- Memindahkan contoh application token dari schema component ke request contoh operasi, lalu merumuskan constraint “minimal satu allowlist berisi ID” dengan `not` JSON Schema agar generator dan validator menerima contoh valid. Semantik tetap mewajibkan `device_ids` dan `target_ids` hadir serta melarang keduanya kosong.

## Berkas

- `package.json`, `pnpm-lock.yaml`, `apps/portal/package.json`
- `apps/portal/src/api/client.ts`, `apps/portal/src/api/generated.ts`
- `contracts/openapi.yaml`, `.github/workflows/ci.yml`, `.prettierignore`
- `docs/execution/backlog.json`, `docs/execution/WORKLOG.md`

## Verifikasi

Tanggal seluruh pemeriksaan: 19 Oktober 2026.

| Perintah / pemeriksaan                                         | Hasil                                                                                                            |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `pnpm contracts:check`                                         | PASS; Redocly memvalidasi OpenAPI/contoh, generator menghasilkan types, dan generated diff check selesai sukses. |
| `pnpm contracts:generate` dua kali dan SHA-256 file            | PASS; kedua hasil identik: `a477a001547a95020c0b7a4e9ab0c4f915f2f882ee06d467f9562a73fcb1a1c1`.                   |
| `pnpm typecheck`                                               | PASS; generated `paths` terpakai dalam client dan seluruh portal typecheck lulus.                                |
| `pnpm lint`                                                    | PASS; ESLint dan Prettier. `.prettierignore` mengecualikan lockfile pnpm yang formatnya dikelola pnpm.           |
| `pnpm test`                                                    | PASS; 1 file, 4 tes portal.                                                                                      |
| `pnpm build`                                                   | PASS; Vite menghasilkan build production.                                                                        |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | PASS; 11 test tracker.                                                                                           |
| `python3 scripts/project_tracker.py validate`                  | PASS; 39 task, dependensi, evidence paths, planning dan skill coverage.                                          |
| `python3 scripts/project_tracker.py render`                    | PASS; STATUS.md dan BACKLOG.md dihasilkan dari backlog.                                                          |

## Batas

Pemeriksaan di sini mencakup contract lint/generation dan integrasi build/type portal. Belum ada endpoint server nyata atau auth/tenant runtime untuk diuji; hal itu berada pada task backend berikutnya. CI diff check membandingkan generated file yang dilacak dalam checkout CI; working tree lokal proyek saat pemeriksaan masih menyajikan berkas repository sebagai untracked.

## Handoff

P02 selesai 3/3. Task siap berikutnya: DB-01 (schema/migrations dan tenant isolation), sesuai urutan backlog.
