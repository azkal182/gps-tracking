# Backend Tracker — agent instructions

## Start here

- Work only in this repository; project skills live in `.agents/skills`, never install them globally.
- Read `docs/execution/STATUS.md`, then the selected task in `docs/execution/backlog.json`. Check `git status --short` and actual files before assuming recorded progress is current.
- Product baseline: `docs/README.md` and `docs/gps-tracking-baseline-final.md`. Load only the domain docs relevant to the task using `$gps-plan`.
- When asked to continue implementation without a task ID, select the earliest dependency-ready MVP task with `python3 scripts/project_tracker.py next`. A task request authorizes its necessary local implementation, tests and progress updates; backlog order does not authorize production actions.
- Current source of execution truth is backlog.json. STATUS.md and BACKLOG.md are generated views, not independent state. Planning FINAL never means code DONE.

## Local skill routing

| Work | Skill |
|---|---|
| Interpret/change planning, trace requirements | `$gps-plan` |
| Select/resume tasks, record evidence/handoff | `$gps-backlog` |
| REST/OpenAPI/WS schemas, generated client | `$gps-contracts` |
| Login/MFA, tenant, membership/grants, credentials | `$gps-auth` |
| SQL migrations/RLS/locking/dedupe/retention | `$gps-storage` |
| Sessions, ingestion, heartbeat, history, realtime | `$gps-tracking` |
| React/Vite portal, forms, tables, map/history | `$gps-portal` |
| Foundation infrastructure, tests/load/backup/release gates | `$gps-release` |

Skills are instructions, not executable commands. If not listed in the current skill picker, open the corresponding `.agents/skills/<name>/SKILL.md` directly; do not copy it to a global directory.

## Implement within the baseline

- Rust/Axum/Tokio/SQLx/PostgreSQL/PostGIS backend; React/Vite/TypeScript/Tailwind/shadcn portal in the same repo. Use pinned compatible dependencies and existing manifests/scripts when available.
- Preserve tenant and object authorization in services, SQL/RLS and WS. Platform privileges never imply location access. Headers, IDs and UI hiding do not authorize an operation.
- Login session, tracking session, device token and application token have different lifecycles. Follow management API and technical decisions; do not invent shortcuts around MFA/recovery.
- Positions belong to device, target association is temporal. Preserve session_only, future retry, late-arrival, dedupe, commit-before-stored and monotonic latest semantics.
- Adaptive position uploads are distinct from heartbeat and position freshness. Heartbeat never adds a position or extends a tracking session.
- Keep tokens, coordinates, passwords, cookies and recovery codes out of logs, reports and persisted UI state. Test with synthetic fixtures.
- Do not widen scope to MQTT/hardware/analytics or reselect the agreed stack during MVP. Resolve ordinary implementation details autonomously; log design-impact changes in `docs/execution/DECISIONS.md` and update affected contracts/specs before relying on the changed behavior. User instructions take precedence over baseline.
- If requirements contradict, identify the exact clauses and resolve the smallest local ambiguity with an explicit decision. Ask only when the answer changes user intent, external commitments or authorization that cannot be inferred.

## Task completion and continuity

1. Inspect task dependencies and scope. Set `in_progress`, owner and started_at in backlog.json before coding. A future task is not blocked merely because dependencies are unfinished.
2. Implement the smallest complete task; do not mark scaffolds/stubs/mocks as implemented features. Preserve unrelated changes.
3. Run checks appropriate to actual changes. For auth/storage/realtime include relevant negative and concurrency cases. Never label unrun, failed or skipped checks as passed.
4. Record files, exact commands, outcomes, limitations and next action in `docs/execution/evidence/<task-id>.md`; never fabricate a commit hash or environment result.
5. Mark `done` only when all task acceptance criteria and dependencies are satisfied and evidence exists. Otherwise retain in_progress or use blocked with reason/unblock action. Update `docs/execution/WORKLOG.md` and handoff in backlog.json.
6. Run `python3 scripts/project_tracker.py validate` then `python3 scripts/project_tracker.py render`. Report task IDs completed, tests, gaps and next task.

Backlog updates do not modify frozen baseline status. Update execution files even when interrupted or blocked, without claiming work was completed. Reopen a done task with explicit reason/evidence if new failures invalidate its acceptance; inspect downstream done tasks as well.

## Verification commands

Available now: `python3 scripts/project_tracker.py validate`, `status`, `next`, `render`, and `python3 -m unittest discover -s scripts/tests -p 'test_*.py'`.

After scaffold exists, inspect manifests/CI before using `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and portal pnpm scripts. Do not pretend commands or applications exist before foundation tasks create them. Migration/contract/E2E/load/restore tasks must establish reproducible commands and evidence.

## Scope and permissions

- The backlog is not permission to deploy, buy services, send invitations/email to real users, delete production data or rotate live credentials. Use local disposable services and fixtures for development; retain any authorization already given by the user.
- Do not create global Codex settings/skills or change other projects. Do not commit/push unless requested or authorized by session context.
- Do not spawn agents merely because this is agentic development. If the user explicitly requests parallel work, allocate disjoint task/file ownership and reconcile tracker updates; never overwrite another agent's evidence.
- Communicate in Indonesian unless requested otherwise. Keep identifiers/code English and explain concrete behavior, test results and material limitations.
