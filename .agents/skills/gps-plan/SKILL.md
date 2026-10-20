---
name: gps-plan
description: "Read and apply this repository\u2019s GPS tracking planning; use for requirement lookup, design decisions, plan audits and impact analysis before changing the agreed MVP."
---

# GPS Planning

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [README.md](../../../docs/README.md)
- [gps-tracking-baseline-final.md](../../../docs/gps-tracking-baseline-final.md)
- [gps-tracking-app-spec.md](../../../docs/gps-tracking-app-spec.md)
- [gps-tracking-technical-decisions.md](../../../docs/gps-tracking-technical-decisions.md)

## Workflow

Read `references/planning-map.md` to select source docs. Do not load all specs for a narrow task.

1. Map the request to requirement/acceptance IDs and relevant backlog task. Check implementation evidence before claiming a feature exists.
2. Read the owning document and dependent contract. Product meaning belongs to App/Portal Spec; wire behavior to API docs; technical choices to Technical Decisions; task status to backlog.json.
3. Preserve final decisions. For a necessary correction, state the conflicting clauses, choose the smallest change consistent with user intent, log it in execution/DECISIONS.md and update dependent specs/contracts/acceptance. Do not restart settled product discussions for routine implementation details.
4. Keep measured facts, configurable defaults, acceptance targets and unimplemented deliverables separate. FINAL is planning status only.
5. Before handoff, show references/decision and implementation impact; run tracker validation when links/coverage change.

Every baseline document is routed in the map; do not duplicate its full contents into skills.
