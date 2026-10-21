# PORTAL-05 — Portal detail dan lifecycle target/device yang lengkap

Tanggal: 2026-10-21
Status: selesai

## Hasil

Target dapat dibuka sebagai detail tenant-scoped, dicari lewat API, dan dibuat/diedit dengan `name`, `external_ref`, serta metadata JSON. Device dapat diedit untuk `name`, `metadata`, `ingestion_policy`, dan `enabled`; protocol dan identifier tetap immutable. Detail device ditampilkan dari response list resmi yang sudah memuat metadata dan adapter identity.

Daftar target/device menggunakan filter nama serta cursor API. Daftar tracking session menggunakan filter status dan cursor. Daftar credential menampilkan metadata saja, memiliki pagination, dan mendukung revoke dengan recent authentication; token hanya tampil sekali setelah rotasi. Disable device dan akhir tracking session meminta konfirmasi serta menyatakan dampak terhadap credential dan histori. Tidak ada hard delete target/device/session.

## Evidence dan verifikasi

- `pnpm --dir apps/portal typecheck` — lulus.
- `pnpm --dir apps/portal lint` — lulus.
- `pnpm --dir apps/portal build` — lulus. Vite tetap memberi warning ukuran vendor MapLibre; file tracking-management terpisah dan tidak mengubah bundle peta.
- `pnpm --dir apps/portal test` — lulus, 9 file / 41 test.
- Tes baru mencakup pencarian/detail target, edit field device mutable, credential metadata tanpa menampilkan secret, credential revoke + recent authentication, konfirmasi disable, serta token rotasi one-time. Tes grant yang ada memastikan aksi credential hanya tersedia sesuai grant.
- `pnpm contracts:lint` — lulus; endpoint yang digunakan sudah ada pada kontrak.
- `cargo fmt --all -- --check`, `cargo test -p server --no-run`, dan `cargo test -p server` — lulus pada hasil yang sama dengan PORTAL-04; 15 tes database integration dilewati karena tidak ada database disposable.
- `git diff --check` — lulus.
- `python3 scripts/project_tracker.py validate` — lulus setelah backlog diperbarui.

## Batas verifikasi

Portal tests menggunakan fixture API, bukan backend/database hidup. Tenant grants, cursor binding, credential revocation, disable atomicity, dan session lifecycle tetap dijamin oleh endpoint backend serta belum diuji ulang terhadap PostgreSQL pada task ini. Vite masih menampilkan warning ukuran bundle vendor MapLibre yang sudah tercatat pada evidence PORTAL sebelumnya.
