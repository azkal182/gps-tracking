# FOUND-01 — Scaffold workspace Rust dan health

Tanggal: 19 Oktober 2026. Status: `done`.

## Perubahan

- Menambahkan Cargo workspace resolver 2 dengan `apps/server` dan crate boundaries `tracking-core`, `tracking-protocols`, `tracking-storage`, serta `tracking-realtime` sesuai baseline.
- Menambahkan versi dependency langsung yang dipin di workspace manifest dan menghasilkan `Cargo.lock` dari resolusi Cargo.
- Server Axum menyediakan `/healthz` dan `/readyz`; readiness dapat diturunkan untuk mengembalikan 503.
- Config environment typed untuk bind address dan optional `DATABASE_URL`. `Secret` menyamarkan `Debug`; error konfigurasi tidak memuat nilai input.
- Axum graceful shutdown menangani SIGINT/SIGTERM. Logging startup hanya menyertakan alamat bind, tanpa config/secret.
- README server menyatakan batas readiness proses-level. Integrasi pemeriksaan database dikerjakan di task storage berikutnya; saat ini readiness tidak mengklaim database reachable.
- Tes unit memverifikasi response health/readiness dan redaksi secret.

## Bukti verifikasi

| Perintah | Hasil |
|---|---|
| `cargo fmt --all` | PASS; formatter diterapkan |
| `cargo fmt --all -- --check` | PASS setelah formatting |
| `cargo metadata --no-deps --format-version 1` | PASS; mengenali server dan empat workspace crates |
| `cargo test --workspace` (dengan akses registry) | PASS; 2 tes server lulus, crate lain dan doctest tanpa tes gagal |
| `cargo build --workspace` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo metadata --no-deps --format-version 1` | PASS; seluruh lima package dikenali |
| `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` | PASS; 11 tracker tests |
| `python3 scripts/project_tracker.py validate` | PASS; 39 tasks, dependency/evidence/coverage validation |

## Batas dan langkah berikutnya

Acceptance FOUND-01 terpenuhi. `/readyz` saat ini hanya mencerminkan kesiapan proses; hubungkan dengan pemeriksaan database di task storage sebelum service mengklaim dependency readiness.

Tidak ada deployment, koneksi database, ataupun klaim fitur tracking yang dilakukan pada task ini.
