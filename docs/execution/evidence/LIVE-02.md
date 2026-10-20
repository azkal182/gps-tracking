# LIVE-02 — WS ticket, subscriptions dan timeout

Status: `done`
Tanggal: 2026-10-19

## Hasil implementasi

- Menambahkan `POST /v1/realtime/tickets` dan `GET /v1/ws/tracking`. Portal session menggunakan Origin + CSRF; aplikasi dapat meminta ticket dengan scope `tracking:subscribe` atau melakukan handshake langsung memakai application bearer. Device bearer tidak dapat membuka stream.
- Ticket acak disimpan sebagai digest, terikat principal dan tenant, berlaku maksimal 60 detik dan dikonsumsi atomik sekali. Ticket berada di memori proses, sehingga restart menginvalidasi ticket lama. Browser Origin dicocokkan persis dengan konfigurasi public origin.
- Subscription target/device divalidasi sebagai satu permintaan: UUID canonical, duplikasi, batas total 100, keberadaan resource dalam tenant, dan grant/scope terkini diperiksa sebelum set diganti dan ack dikirim. Permintaan invalid tidak mengubah subscription sebelumnya.
- Fan-out mengirim event posisi hanya ketika ingestion berhasil commit dan latest position benar-benar maju; event sesi diterbitkan setelah transaksi sesi commit. Status konektivitas dan freshness posisi diproyeksikan oleh worker per tenant setiap lima detik, tanpa membuat position atau memperpanjang session.
- Setiap event difilter berdasarkan tenant dan subscription, lalu principal, membership, resource dan grant/scope divalidasi ulang. Koneksi ditutup saat login session/application token/membership/grant tidak lagi valid. Batas 5 koneksi per user, antrean 256 event per socket, timeout send 10 detik, serta penanganan slow consumer dengan menandai resync dan menutup socket diterapkan.
- Ditemukan assertion migrasi database yang masih mengharapkan 11 migrasi meski repo berisi 13. Angka di tes diperbarui agar seluruh schema migration LIVE-02 dan sebelumnya tetap diverifikasi secara tepat.

## Evidence dan validasi

- `cargo fmt --all -- --check` — lulus.
- `cargo clippy --workspace --all-targets -- -D warnings` — lulus.
- `cargo test --workspace` — lulus: 13 unit tests passed; 13 server DB tests dan 4 storage DB tests ignored pada mode tanpa env DB.
- `GPS_TEST_DATABASE_URL=<disposable PostGIS> cargo test --workspace -- --ignored --test-threads=1` — lulus pada container PostGIS disposable: 13/13 server integration tests dan 4/4 storage/migration integration tests. Kasus LIVE-02 mencakup Origin salah, ticket expired/replay/restart, session login expired sebelum handshake, tenant/resource tak berizin dengan subscription lama tetap aktif, event connectivity online/offline, application bearer ticket dan direct bearer, pencabutan token/session/grant saat WS terbuka.
- `GPS_TEST_DATABASE_URL=<disposable PostGIS> cargo test -p server --lib live02_websocket_ticket_subscription_and_revoke -- --ignored --test-threads=1` — lulus: 1/1, termasuk expired login session.
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` — lulus: 11 tests.
- `pnpm contracts:check` — lulus; OpenAPI valid dan generated TypeScript tidak drift.
- `python3 scripts/project_tracker.py validate` — lulus sebelum perubahan status final; render dijalankan setelah backlog diperbarui.
- `git diff --check` — lulus sebelum perubahan dokumen final; dijalankan ulang setelahnya.

## Batasan yang diketahui

- Hub event, ticket dan koneksi bersifat process-local; deployment multi-instance memerlukan koordinasi/pub-sub lintas instance sebelum dijalankan sebagai beberapa replica. Ini sesuai baseline MVP single-instance.
- Worker status berjalan hanya ketika konfigurasi tracking database tersedia; startup baseline tidak menghasilkan transisi palsu. Tidak ada durable event replay; client perlu melakukan resync melalui REST setelah reconnect/slow consumer.
- Beban 100 device pada dua organisasi belum diuji sebagai benchmark/load test; pengujian ini membuktikan correctness protokol dan jalur database pada fixture, bukan kapasitas produksi.

## Handoff

LIVE-02 memenuhi acceptance task. Berikutnya ambil task dependency-ready melalui `python3 scripts/project_tracker.py next` (diperkirakan LIVE-03).
