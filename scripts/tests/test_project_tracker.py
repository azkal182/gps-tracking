import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('tracker', Path(__file__).parents[1] / 'project_tracker.py')
tracker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tracker)


class TrackerTests(unittest.TestCase):
    def setUp(self):
        task = dict(id='A', phase='P01', title='A', scope='mvp', status='todo', depends_on=[],
                    skills=['gps-backlog'], references=['docs/README.md'], acceptance=['Verified behavior'],
                    owner=None, started_at=None, completed_at=None, evidence=[])
        second = copy.deepcopy(task)
        second.update(id='B', depends_on=['A'])
        self.data = dict(schema_version=1, updated_at='2026-10-19', phases=[dict(id='P01', order=1, title='Build')],
                         tasks=[task, second], handoff=dict(current_task=None, next_task='A', summary='Not started', next_action='A'))

    def done(self, t):
        t.update(status='done', owner='tester', started_at='2026-10-19', completed_at='2026-10-19', evidence=['proof.md'])

    def test_dependency_readiness(self):
        self.assertEqual([t['id'] for t in tracker.ready_tasks(self.data)], ['A'])
        self.done(self.data['tasks'][0])
        self.assertEqual([t['id'] for t in tracker.ready_tasks(self.data)], ['B'])

    def test_reject_done_without_evidence(self):
        self.done(self.data['tasks'][0]); self.data['tasks'][0]['evidence'] = []
        self.assertTrue(any('evidence' in e for e in tracker.validate(self.data, check_files=False)))

    def test_reject_done_before_dependencies(self):
        self.done(self.data['tasks'][1])
        self.assertTrue(any('dependency A not done' in e for e in tracker.validate(self.data, check_files=False)))

    def test_reject_cycles_and_unknown_dependencies(self):
        self.data['tasks'][0]['depends_on'] = ['B']
        self.assertTrue(any('cycle' in e for e in tracker.validate(self.data, check_files=False)))
        self.data['tasks'][0]['depends_on'] = ['MISSING']
        self.assertTrue(any('unknown/self' in e for e in tracker.validate(self.data, check_files=False)))

    def test_reject_deferred_mvp(self):
        self.data['tasks'][0]['status'] = 'deferred'
        self.assertTrue(any('cannot be deferred' in e for e in tracker.validate(self.data, check_files=False)))

    def test_blocker_requires_unblock_action(self):
        self.data['tasks'][0].update(status='blocked', blocked_reason='No DB', unblock_action=None)
        self.assertTrue(any('unblock action' in e for e in tracker.validate(self.data, check_files=False)))

    def test_no_false_mvp_claim_from_setup(self):
        self.assertIn('0/2 task done', tracker.summary(self.data))
        self.assertIn('Implementasi MVP belum selesai', tracker.summary(self.data))

    def test_repository_data_validates(self):
        data = json.loads((tracker.ROOT / 'docs/execution/backlog.json').read_text())
        self.assertEqual(tracker.validate(data), [])

    def test_malformed_references_fail_without_crashing(self):
        self.data['tasks'][0]['references'] = 'not-an-array'
        self.assertTrue(any('references requires' in e for e in tracker.validate(self.data)))

    def test_pilot_gate_stays_incomplete(self):
        self.data['phases'].append(dict(id='P11', order=11, title='Pilot'))
        self.data['tasks'][1]['phase'] = 'P11'
        self.done(self.data['tasks'][0])
        self.assertIn('Siap deployment; pilot belum selesai', tracker.summary(self.data))
        self.done(self.data['tasks'][1])
        self.assertIn('MVP selesai (berdasarkan evidence', tracker.summary(self.data))

    def test_nonlocal_paths_rejected(self):
        self.assertFalse(tracker.local_file(tracker.ROOT, '/etc/passwd'))
        self.assertFalse(tracker.local_file(tracker.ROOT, '../outside'))


if __name__ == '__main__':
    unittest.main()
