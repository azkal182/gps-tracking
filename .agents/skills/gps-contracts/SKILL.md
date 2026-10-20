---
name: gps-contracts
description: "Create or change OpenAPI, REST schemas, WebSocket schemas or generated TypeScript clients for this repository\u2019s tracking and management APIs."
---

# GPS Contracts

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-api-contract.md](../../../docs/gps-tracking-api-contract.md)
- [gps-tracking-management-api.md](../../../docs/gps-tracking-management-api.md)
- [gps-tracking-technical-decisions.md](../../../docs/gps-tracking-technical-decisions.md)

## Workflow

Use both tracking and management API documents when auth/tenant fields cross boundaries. Before implementation, enumerate affected paths, status codes, request/response fields, auth mode, scope, tenant binding and acceptance cases.

- Write/maintain contracts/openapi.yaml and explicit WS schemas; generate TypeScript client from the source contract. Do not hand-edit generated client or treat TS types as runtime validation.
- Portal cookie+CSRF, integration bearer and device bearer are separate security modes. Credential/account management cannot be reached by integration tokens. Header tenant is verified, never trusted alone.
- Keep tenant_id, observed_at, decimal-string position ID and distinct tracking/connectivity/position states in schema and examples. Session create requires Idempotency-Key; replay must reauthorize.
- Preserve cursor binding and range/limit rules; do not introduce arbitrary page numbers or sorts unsupported by SQL.
- Validate specs/examples and generated diff; exercise malformed input, cross-tenant access, wrong principal and documented error mapping. Tool command must exist in manifests before reporting success.
- A planned path is not an implemented endpoint. Record contract and runtime checks separately in task evidence.
