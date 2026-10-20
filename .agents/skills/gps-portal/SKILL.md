---
name: gps-portal
description: "Build this repository\u2019s React/Vite portal for management, provisioning, auth, live maps and history using its agreed frontend stack and API contracts."
---

# GPS Portal

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-portal-spec.md](../../../docs/gps-tracking-portal-spec.md)
- [gps-tracking-frontend-stack.md](../../../docs/gps-tracking-frontend-stack.md)
- [gps-tracking-management-api.md](../../../docs/gps-tracking-management-api.md)
- [gps-tracking-api-contract.md](../../../docs/gps-tracking-api-contract.md)
- [gps-tracking-technical-decisions.md](../../../docs/gps-tracking-technical-decisions.md)

## Workflow

Read the page's FR-PORT/AC-PORT and its API contract. Use React/TypeScript/Vite/Tailwind/shadcn plus Router/Query/RHF/Zod; library presence is verified in actual manifests, not assumed from planning.

- Query owns server state; RHF owns forms; URL owns safe shareable filters; local state owns small interactions. Add Zustand only for concrete cross-component UI needs. Do not persist coordinates or secret tokens.
- Bind query keys, in-flight work and WS to tenant. Clear sensitive context when switching org/logout and reject late responses for a previous context.
- Credential secret is displayed once and held transiently; register device does not start tracking automatically. UI permissions reflect backend grants but never replace them.
- MapLibre/MapTiler map shows sample lines, timestamp/freshness/connectivity separately, loading/empty/offline/error and partial-history labels. Keep table usable if tiles fail; no silent full-year fetch or route inference.
- Use cursor navigation/server-supported filters. Preserve UTC conversion with explicit timezone labels. Integrate WS changes without refetching all history for every heartbeat.
- Verify forms, accessible interaction, wrong role/tenant, MFA, secret handling, request races, map fallback and history pagination in meaningful component/E2E tests. Use local synthetic fixtures, not live device data.
