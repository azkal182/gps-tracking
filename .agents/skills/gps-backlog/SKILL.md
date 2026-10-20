---
name: gps-backlog
description: "Select, resume or update this repository\u2019s GPS MVP tasks, phases, completion evidence and handoff. Use when asked to continue development or report actual progress."
---

# GPS Backlog

Use only for this repository. Resolve linked docs relative to this file; run project commands from repository root.

## Sources

- [gps-tracking-implementation-plan.md](../../../docs/gps-tracking-implementation-plan.md)
- [gps-tracking-baseline-final.md](../../../docs/gps-tracking-baseline-final.md)

## Workflow

Read root AGENTS.md and docs/execution/WORKFLOW.md, then use `python3 scripts/project_tracker.py status` and `next` from the repository root.

- Resume in_progress before claiming a new task. Inspect owner/worktree changes; do not silently take over concurrent work. Pick the earliest todo with all dependencies done when the user has not named a task.
- Read the JSON task's scope, acceptance, refs and skills. Move to in_progress with owner/started_at, implement the authorized scope, and invoke/read only domain skills needed.
- Update evidence at docs/execution/evidence/<ID>.md with actual commands/outcomes/files. Do not mark done on a draft, mock, missing acceptance or unrun check. For external blockers record reason and next action; continue independent authorized tasks when useful.
- Update backlog.json, WORKLOG.md and handoff before ending a work turn. Run validate then render. Never edit generated STATUS/BACKLOG by hand.
- Explain done counts as tasks, not time estimates. Exclude P00 and POST from MVP implementation counts. All required P01–P11 tasks must be done for MVP complete; P11 deployment requires actual authorization/environment.
- Read-only status requests do not require changing owner/state. Dependency not done means not-ready, not blocked.

This skill does not authorize production writes, purchases or external messaging.
