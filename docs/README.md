# GPS Tracking — indeks planning

> **Status:** FINAL — Perencanaan MVP v1.0 (19 Oktober 2026). Final sebagai acuan implementasi; bukan klaim implementasi selesai atau siap produksi.

Makna status final dan batas validasi: [Baseline Final](./gps-tracking-baseline-final.md).

## Mulai di sini

1. [Backend Summary](./gps-tracking-backend-summary.md): cakupan ringkas.
2. [App Spec](./gps-tracking-app-spec.md): kebutuhan domain, invariant dan acceptance.
3. [Portal Spec](./gps-tracking-portal-spec.md): alur management, peta/histori, peran.
4. [Technical Decisions](./gps-tracking-technical-decisions.md): keputusan auth, frontend, onboarding, peta dan deployment.
5. [Tracking API](./gps-tracking-api-contract.md) dan [Management API](./gps-tracking-management-api.md): route, schema perencanaan, status dan autentikasi. OpenAPI tracking/management serta schema pesan WebSocket berada di [contracts/openapi.yaml](../contracts/openapi.yaml) dan divalidasi pada task kontrak P02.
6. [Database & Operations](./gps-tracking-database-operations.md): skema logis, constraints, locking, retensi, backup dan batas beban.
7. [Frontend Stack](./gps-tracking-frontend-stack.md): library dan pembagian state.
8. [Implementation Plan](./gps-tracking-implementation-plan.md): urutan pekerjaan beserta release gates.

## Status keputusan

- React/Vite/TypeScript, Tailwind, shadcn; Router/Query/RHF/Zod, typed API client, TanStack Table. Zustand kondisional.
- Akun internal, cookie server session + CSRF; TOTP untuk MFA, recovery codes; undangan tanpa public registration.
- Organisasi dikelola platform, membership/grants terpisah; platform tidak otomatis berhak membaca lokasi.
- Target/device/session tetap P0; session_only default; adaptive positions + heartbeat 60s/offline 180s.
- Posisi satu tahun kalender; audit 365 hari; backup recovery window 30 hari dengan dependency maksimal 37 hari.
- MapLibre + MapTiler, histori dibatasi per query, single-instance Rust/portal dengan PostgreSQL/PostGIS.

Technical Decisions menetapkan pilihan desain; domain requirements tetap App Spec dan kontrak antarmuka mengikuti dua API docs. Jika ditemukan konflik, perbaiki dokumen spesifik sebelum implementasi terkait, jangan mengandalkan perilaku tersembunyi. OpenAPI adalah kontrak implementasi; DDL tetap deliverable fase database.

## Audit konsistensi perencanaan

- Portal sudah masuk MVP; tidak lagi dikecualikan sebagai UI.
- Tracking active/inactive terpisah dari connectivity online/offline/unknown dan position fresh/stale/no_position.
- Cookie portal dan bearer integration/device dijelaskan terpisah; CSRF dan konteks tenant eksplisit.
- Token device dapat dicabut/dirotasi; suspend dan reaktivasi tidak menghidupkan secret lama.
- Kontrak management, keputusan MFA, peta, frontend, retensi audit/backup serta task breakdown tersedia.
- Konten JSON contoh, tautan lokal dan Markdown fences diperiksa. Ini bukan pengujian aplikasi atau pembuktian capacity/security.

Deployment membutuhkan domain/host/storage dan credentials SMTP/tile/backup. Tidak ada pembelian layanan atau deploy dilakukan oleh pekerjaan perencanaan ini.

## Pengembangan dengan AI agent

Instruksi repository: [AGENTS.md](../AGENTS.md). Skill lokal berada di `.agents/skills` dan merujuk planning di atas. Status pelaksanaan berbeda dari status planning final.

- [Status terkini](./execution/STATUS.md)
- [Backlog fase sampai MVP](./execution/BACKLOG.md)
- [Workflow dan contoh pemakaian skill](./execution/WORKFLOW.md)
- [Worklog](./execution/WORKLOG.md) dan [execution decisions](./execution/DECISIONS.md)

Gunakan `python3 scripts/project_tracker.py status` untuk memeriksa progres aktual. Tugas setup/planning tidak dihitung sebagai implementasi produk.

Panduan layanan dan quality checks lokal: [Environment development](./execution/development-environment.md).
Panduan deploy staging ke VPS dan CI/CD GitHub: [Staging VPS & GitHub Actions](./staging-vps-github-cicd.md).
