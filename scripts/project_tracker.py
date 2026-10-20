#!/usr/bin/env python3
"""Validate and display repository execution state. No network or non-stdlib packages."""
import argparse
import json
from collections import Counter
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATES = {'todo', 'in_progress', 'blocked', 'done', 'deferred'}
SCOPES = {'setup', 'mvp', 'post_mvp'}


def local_file(root, value):
    if not isinstance(value, str) or not value or Path(value).is_absolute():
        return False
    path = (root / value).resolve()
    return path.is_relative_to(root.resolve()) and path.is_file()


def valid_date(value):
    try:
        date.fromisoformat(value)
        return True
    except (TypeError, ValueError):
        return False


def validate(data, root=ROOT, check_files=True):
    errors = []
    if not isinstance(data, dict) or data.get('schema_version') != 1:
        return ['schema_version must be 1']
    if not valid_date(data.get('updated_at')):
        errors.append('updated_at must be ISO date')
    phases = data.get('phases', [])
    tasks = data.get('tasks', [])
    if not isinstance(phases, list) or not isinstance(tasks, list) or not tasks:
        return errors + ['phases/tasks must be nonempty arrays']
    phase_ids = [p.get('id') for p in phases if isinstance(p, dict)]
    if len(phase_ids) != len(phases) or len(set(phase_ids)) != len(phase_ids):
        errors.append('duplicate or malformed phases')
    ids = [t.get('id') for t in tasks if isinstance(t, dict)]
    if len(ids) != len(tasks) or any(not isinstance(i, str) or not i for i in ids):
        return errors + ['each task requires a nonempty string ID']
    if len(set(ids)) != len(ids):
        errors.append('duplicate task IDs')
    by_id = {t['id']: t for t in tasks}
    for t in tasks:
        key = t['id']
        for field in ('title',):
            if not isinstance(t.get(field), str) or not t[field].strip():
                errors.append(f'{key}: missing {field}')
        if t.get('phase') not in phase_ids:
            errors.append(f'{key}: unknown phase')
        if t.get('status') not in STATES or t.get('scope') not in SCOPES:
            errors.append(f'{key}: invalid scope/status')
        scope = 'setup' if t.get('phase') == 'P00' else ('post_mvp' if t.get('phase') == 'POST' else 'mvp')
        if t.get('scope') != scope:
            errors.append(f'{key}: phase/scope mismatch')
        if t.get('status') == 'deferred' and t.get('scope') != 'post_mvp':
            errors.append(f'{key}: required task cannot be deferred')
        for field in ('acceptance', 'skills', 'references'):
            if not isinstance(t.get(field), list) or not t[field] or any(not isinstance(v, str) or not v.strip() for v in t[field]):
                errors.append(f'{key}: {field} requires nonempty strings')
        deps = t.get('depends_on')
        if not isinstance(deps, list) or any(not isinstance(d, str) for d in deps):
            errors.append(f'{key}: invalid dependencies')
            deps = []
        if len(set(deps)) != len(deps):
            errors.append(f'{key}: duplicate dependency')
        for dep in deps:
            if dep not in by_id or dep == key:
                errors.append(f'{key}: unknown/self dependency {dep}')
            elif t.get('status') == 'done' and by_id[dep].get('status') != 'done':
                errors.append(f'{key}: done but dependency {dep} not done')
        if t.get('status') in {'in_progress', 'done'}:
            if not t.get('owner') or not valid_date(t.get('started_at')):
                errors.append(f'{key}: active/done requires owner and started_at')
        if t.get('status') == 'blocked':
            if not t.get('blocked_reason') or not t.get('unblock_action'):
                errors.append(f'{key}: blocked requires reason and unblock action')
        evidence = t.get('evidence')
        if not isinstance(evidence, list) or any(not isinstance(x, str) for x in evidence):
            errors.append(f'{key}: evidence must be an array of paths')
            evidence = []
        if t.get('status') == 'done':
            if not valid_date(t.get('completed_at')) or not evidence:
                errors.append(f'{key}: done requires completed_at and evidence')
            elif valid_date(t.get('started_at')) and t['completed_at'] < t['started_at']:
                errors.append(f'{key}: completion precedes start')
        elif t.get('completed_at'):
            errors.append(f'{key}: non-done task has completed_at')
        if check_files:
            refs = t.get('references') if isinstance(t.get('references'), list) else []
            for path in refs + evidence:
                if not local_file(root, path):
                    errors.append(f'{key}: missing/nonlocal reference {path}')
                elif path in evidence and not (root / path).read_text().strip():
                    errors.append(f'{key}: empty evidence {path}')
            names = t.get('skills') if isinstance(t.get('skills'), list) else []
            for name in names:
                if not local_file(root, f'.agents/skills/{name}/SKILL.md'):
                    errors.append(f'{key}: missing skill {name}')
    visiting, visited = set(), set()

    def visit(key):
        if key in visiting:
            errors.append(f'dependency cycle at {key}')
            return
        if key in visited:
            return
        visiting.add(key)
        deps = by_id[key].get('depends_on')
        for dep in deps if isinstance(deps, list) else []:
            if isinstance(dep, str) and dep in by_id:
                visit(dep)
        visiting.remove(key)
        visited.add(key)

    for key in by_id:
        visit(key)
    handoff = data.get('handoff', {})
    if not isinstance(handoff, dict):
        errors.append('handoff must be an object')
    else:
        for field in ('current_task', 'next_task'):
            if handoff.get(field) is not None and handoff[field] not in by_id:
                errors.append(f'handoff: unknown {field}')
        if not handoff.get('summary') or not handoff.get('next_action'):
            errors.append('handoff summary/next_action required')
    if check_files:
        if not local_file(root, data.get('baseline')):
            errors.append('baseline file missing')
        coverage_path = root / 'docs/execution/skill-coverage.json'
        try:
            coverage = json.loads(coverage_path.read_text())['planning']
            expected = {p.relative_to(root).as_posix() for p in (root / 'docs').glob('*.md')}
            if set(coverage) != expected:
                errors.append('skill coverage must map every top-level planning document exactly')
            for path, names in coverage.items():
                if not local_file(root, path) or not names:
                    errors.append(f'invalid coverage entry {path}')
                for name in names:
                    if not local_file(root, f'.agents/skills/{name}/SKILL.md'):
                        errors.append(f'coverage: missing skill {name}')
        except (OSError, ValueError, KeyError, TypeError):
            errors.append('skill-coverage.json invalid or missing')
    return errors


def ready_tasks(data):
    by_id = {t['id']: t for t in data['tasks']}
    order = {p['id']: p['order'] for p in data['phases']}
    return sorted((t for t in data['tasks'] if t['scope'] != 'post_mvp' and t['status'] == 'todo'
                   and all(by_id[d]['status'] == 'done' for d in t['depends_on'])),
                  key=lambda t: (order[t['phase']], data['tasks'].index(t)))


def summary(data):
    product = [t for t in data['tasks'] if t['scope'] == 'mvp']
    completed = sum(t['status'] == 'done' for t in product)
    before_pilot = [t for t in product if t['phase'] != 'P11']
    if completed == len(product) and product:
        stage = 'MVP selesai (berdasarkan evidence tercatat; bukan verifikasi otomatis runtime)'
    elif before_pilot and all(t['status'] == 'done' for t in before_pilot):
        stage = 'Siap deployment; pilot belum selesai'
    else:
        stage = 'Implementasi MVP belum selesai'
    lines = ['# Status eksekusi proyek', '', '> Generated dari backlog.json; jangan edit langsung.', '',
             f"Tanggal data: {data['updated_at']}", '', f'**{stage}.**', '',
             f'Implementasi MVP: **{completed}/{len(product)} task done**. P00 dan POST tidak dihitung; ini bukan persentase waktu/effort.', '',
             '| Fase | Done/total | Status |', '|---|---:|---|']
    for phase in sorted(data['phases'], key=lambda p: p['order']):
        items = [t for t in data['tasks'] if t['phase'] == phase['id']]
        counts = Counter(t['status'] for t in items)
        lines.append(f"| {phase['id']} — {phase['title']} | {counts['done']}/{len(items)} | " + ', '.join(f'{n} {s}' for s, n in sorted(counts.items())) + ' |')
    active = [t['id'] for t in data['tasks'] if t['status'] == 'in_progress']
    ready = ready_tasks(data)
    lines += ['', '## Handoff', '', 'Sedang dikerjakan: ' + (', '.join(active) or 'tidak ada'), '',
              'Task siap: ' + (', '.join(t['id'] for t in ready) or 'tidak ada'), '', data['handoff']['summary'], '',
              'Langkah berikut: ' + data['handoff']['next_action']]
    for t in data['tasks']:
        if t['status'] == 'blocked':
            lines += ['', f"Blocked {t['id']}: {t['blocked_reason']} — {t['unblock_action']}"]
    lines += ['', 'Batas verifikasi:', *['- '+x for x in data['handoff'].get('limitations', [])], '',
              'Detail: [BACKLOG](./BACKLOG.md), [workflow](./WORKFLOW.md), [worklog](./WORKLOG.md).', '']
    return '\n'.join(lines)


def backlog_view(data):
    lines = ['# Backlog sampai MVP', '', '> Generated dari backlog.json; update status hanya pada JSON.', '',
             'P00 = setup; P01–P11 wajib MVP; POST di luar MVP. Todo belum tentu dependency-ready.', '']
    for phase in sorted(data['phases'], key=lambda p: p['order']):
        lines += [f"## {phase['id']} — {phase['title']}", '']
        for t in [t for t in data['tasks'] if t['phase'] == phase['id']]:
            lines += [f"### {t['id']} — {t['title']}", '',
                      f"Status: **{t['status']}** · Scope: {t['scope']} · Owner: {t['owner'] or '—'}", '',
                      'Dependensi: ' + (', '.join(t['depends_on']) or 'tidak ada'), '',
                      'Skill: ' + ', '.join('`$'+x+'`' for x in t['skills']), '',
                      'Acceptance:', *['- '+a for a in t['acceptance']], '',
                      'Sumber: ' + ', '.join(f'[{Path(p).name}](../../{p})' for p in t['references']), '',
                      'Evidence: ' + (', '.join(f'[{Path(p).name}](../../{p})' for p in t['evidence']) or 'belum tersedia'), '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['validate', 'status', 'next', 'render'])
    args = parser.parse_args()
    try:
        data = json.loads((ROOT / 'docs/execution/backlog.json').read_text())
    except (OSError, ValueError) as exc:
        parser.exit(1, f'Cannot read backlog: {exc}\n')
    errors = validate(data)
    if errors:
        parser.exit(1, '\n'.join('ERROR: '+x for x in errors)+'\n')
    if args.command == 'validate':
        print(f"VALID: {len(data['tasks'])} tasks; dependencies, evidence paths and planning/skill coverage checked. Runtime evidence not independently verified.")
    elif args.command == 'status':
        print(summary(data))
    elif args.command == 'render':
        (ROOT / 'docs/execution/STATUS.md').write_text(summary(data))
        (ROOT / 'docs/execution/BACKLOG.md').write_text(backlog_view(data))
        print('Rendered STATUS.md and BACKLOG.md from valid backlog.')
    else:
        active = [t for t in data['tasks'] if t['status'] == 'in_progress']
        candidates = active or ready_tasks(data)
        if candidates:
            print(('RESUME' if active else 'NEXT')+': '+candidates[0]['id']+' — '+candidates[0]['title'])
        else:
            print('No active or dependency-ready task. Inspect blocked tasks; do not bypass release gates.')


if __name__ == '__main__':
    main()
