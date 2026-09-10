"""Measurement contracts: missing evidence must never become an acceptance."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zlib

import content
from presentation import analyze, presentations
from run import accepted, counts, plan_repetitions, quantiles, schedule, summarize
from fixtures import verify


class Reports(unittest.TestCase):
    def row(self, iteration, value=100, status='ok', group='content'):
        return {'viewer': 'hashline', 'renderer': 'cairo', 'group': group, 'fixture': 'small',
                'refreshHz': None, 'iteration': iteration, 'warmup': iteration < 0,
                'status': status, 'metrics': {'readableUpperMs': value}}

    def test_warmup_excluded_and_nearest_rank(self):
        rows = [self.row(-1, 9999), *[self.row(i, 100 + i) for i in range(30)]]
        cell = summarize(rows, 30)[0]
        self.assertEqual(cell['metrics']['readableUpperMs']['p95'], 128)
        self.assertEqual(cell['budgets'][0]['status'], 'fail')
        self.assertFalse(cell['acceptance'])
        self.assertEqual(quantiles([1, 2, 3, 4, 5])['p95'], 5)

    def test_short_runs_never_accept(self):
        cell = summarize([self.row(i) for i in range(5)], 5)[0]
        self.assertEqual(cell['budgets'][0]['status'], 'pass')
        self.assertFalse(cell['acceptance'])

    def test_one_missing_run_cannot_pass(self):
        rows = [self.row(i) for i in range(29)] + [self.row(29, None, 'missing')]
        cell = summarize(rows, 30)[0]
        self.assertFalse(cell['complete'])
        self.assertFalse(cell['acceptance'])
        self.assertEqual(cell['budgets'][0]['status'], 'unmeasured')

    def test_group_without_a_target_neither_accepts_nor_blocks(self):
        rows = [self.row(i) for i in range(30)] + [self.row(i, 5, group='stages') for i in range(30)]
        summary = summarize(rows, 30)
        stages = next(cell for cell in summary if cell['group'] == 'stages')
        content = next(cell for cell in summary if cell['group'] == 'content')
        self.assertFalse(stages['budgeted'])
        self.assertFalse(stages['acceptance'])
        self.assertTrue(content['acceptance'])
        self.assertTrue(accepted(summary, True))
        self.assertFalse(accepted([stages], True))
        self.assertFalse(accepted(summary, False))

    def test_idle_window_is_not_repeated_thirty_times(self):
        viewers = [('hashline', 'cairo')]
        items = list(schedule(viewers, ['memory', 'idle'], ['small'], 30))
        self.assertEqual(counts(['memory', 'idle'], 30), {'memory': 30, 'idle': 5})
        self.assertEqual(sum(item[1] == 'memory' for item in items), 31)  # warmup and 30
        self.assertEqual(sum(item[1] == 'idle' for item in items), 6)     # warmup and 5

    def test_per_group_repetition_budgets(self):
        groups = ['content', 'memory', 'idle']
        # A bare count is the old behaviour, and idle keeps its short series.
        self.assertEqual(plan_repetitions('30', groups, 30, 5),
                         {'content': 30, 'memory': 30, 'idle': 5})
        self.assertEqual(plan_repetitions(None, groups, 30, 5),
                         plan_repetitions('30', groups, 30, 5))
        # An hour's budget: the p95 series stays long, the flat one gets short.
        self.assertEqual(plan_repetitions('content=30,memory=8', ['content', 'memory'], 30, 5),
                         {'content': 30, 'memory': 8})
        # A bare number moves the default; named groups still win over it.
        self.assertEqual(plan_repetitions('20,content=30', groups, 30, 5),
                         {'content': 30, 'memory': 20, 'idle': 5})
        # An explicit idle budget is not capped by the default short series.
        self.assertEqual(plan_repetitions('idle=12', ['idle'], 30, 5), {'idle': 12})
        for bad in ('content=0', 'bogus=3', 'content=x', '0'):
            with self.subTest(bad), self.assertRaises(Exception):
                plan_repetitions(bad, groups, 30, 5)

    def test_short_group_budget_is_never_an_acceptance(self):
        def row(group, iteration):
            return {'viewer': 'hashline', 'renderer': 'cairo', 'group': group, 'fixture': 'small',
                    'refreshHz': None, 'iteration': iteration, 'warmup': iteration < 0,
                    'status': 'ok', 'metrics': {'readableUpperMs': 100, 'pssMiB': 20}}
        planned = {'content': 30, 'memory': 8}
        summary = summarize([row('content', i) for i in range(30)]
                            + [row('memory', i) for i in range(8)], planned)
        content_cell = next(cell for cell in summary if cell['group'] == 'content')
        memory_cell = next(cell for cell in summary if cell['group'] == 'memory')
        self.assertTrue(content_cell['complete'] and content_cell['sufficient'])
        # Eight settled readings are a measurement, not an acceptance.
        self.assertTrue(memory_cell['complete'])
        self.assertFalse(memory_cell['sufficient'])
        self.assertFalse(accepted(summary, True))

    def test_idle_cell_accepts_at_five_but_memory_does_not(self):
        def row(group, iteration, metric, value):
            return {'viewer': 'hashline', 'renderer': 'cairo', 'group': group, 'fixture': 'small',
                    'refreshHz': None, 'iteration': iteration, 'warmup': iteration < 0,
                    'status': 'ok', 'metrics': {metric: value}}
        summary = summarize([row('idle', i, 'idleCpuPercent', .1) for i in range(5)]
                            + [row('memory', i, 'pssMiB', 20) for i in range(5)],
                            {'idle': 5, 'memory': 5})
        idle = next(cell for cell in summary if cell['group'] == 'idle')
        memory = next(cell for cell in summary if cell['group'] == 'memory')
        self.assertTrue(idle['complete'])
        self.assertTrue(idle['acceptance'])
        self.assertTrue(memory['complete'])
        self.assertFalse(memory['acceptance'])   # a five-run series is not a p95
        self.assertFalse(memory['sufficient'])
        self.assertFalse(accepted(summary, True))

    def test_interleaved_and_balanced(self):
        viewers = [('hashline', 'cairo'), ('hashline', 'vulkan'), ('viewmd', 'native')]
        items = list(schedule(viewers, ['startup', 'scroll'], ['small'], 2))
        self.assertEqual(len(items), 3 * 3 * 2)  # warmup + 2 passes, two cells
        for start in range(0, len(items), 3):
            self.assertEqual({item[-1] for item in items[start:start + 3]}, set(viewers))
        self.assertNotEqual([item[-1] for item in items[:3]], [item[-1] for item in items[6:9]])

    def test_refresh_rate_is_one_condition_for_entire_run(self):
        items = list(schedule([('hashline', 'cairo'), ('hashline', 'vulkan')],
                              ['startup', 'memory', 'scroll'], ['small'], 2, refresh_hz=60))
        self.assertEqual({item[3] for item in items}, {60})
        self.assertEqual(len(items), 18)

    def test_refresh_mismatch_fails_without_changing_display(self):
        from display import verify_rate
        with patch('display.current_mode', return_value={'connector': 'DP-3', 'refreshHz': 60}):
            with self.assertRaisesRegex(RuntimeError, 'No display configuration changed'):
                verify_rate(120, 'DP-3')
            self.assertEqual(verify_rate(60, 'DP-3')['refreshHz'], 60)


class ContentProof(unittest.TestCase):
    def capture(self, payloads):
        import threading
        capture = object.__new__(content.Capture)
        capture.error = None
        capture.lock = threading.Lock()
        capture.frames = [dict(receivedMonotonicNs=t, ptsNs=t, sha256='test', packed=zlib.compress(text.encode())) for t, text in payloads]
        return capture

    def test_leftover_window_is_dropped_from_the_baseline(self):
        capture = self.capture([(1, 'Lesbarer Text mit'), (2, 'wallpaper')])
        with patch('content.ocr', side_effect=lambda data, psm=6: data.decode()):
            self.assertEqual(capture.clear(['Lesbarer Text mit'])['receivedMonotonicNs'], 2)
            self.assertEqual(len(capture.frames), 1)

    def test_screen_that_never_clears_is_an_error(self):
        capture = self.capture([(1, 'Lesbarer Text mit')])
        with patch('content.ocr', side_effect=lambda data, psm=6: data.decode()):
            with self.assertRaisesRegex(RuntimeError, 'still on screen'):
                capture.clear(['Lesbarer Text mit'], timeout=.01)

    def test_empty_frame_never_counts(self):
        capture = self.capture([(1, 'wallpaper'), (20, 'window title'), (30, 'Lesbarer Text mit Hervorhebung')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda data, psm=6: data.decode()):
            result = capture.proof(10, ['Lesbarer Text mit'], Path(temp))
            self.assertTrue(result['contentVerified'])
            self.assertEqual(result['readableUpperMs'], 20 / 1e6)
            self.assertEqual(len(result['frames']), 3)

    def test_preexisting_text_invalidates(self):
        capture = self.capture([(1, 'Lesbarer Text mit'), (30, 'Lesbarer Text mit')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda data, psm=6: data.decode()):
            with self.assertRaisesRegex(RuntimeError, 'already visible'):
                capture.proof(10, ['Lesbarer Text mit'], Path(temp))

    def test_timeout_is_missing(self):
        capture = self.capture([(1, 'wallpaper'), (20, 'empty window')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda data, psm=6: data.decode()):
            result = capture.proof(10, ['document body'], Path(temp))
            self.assertEqual(result['status'], 'missing')
            self.assertNotIn('readableUpperMs', result)


class Presentation(unittest.TestCase):
    def trace(self, stamps):
        lines = ['wp_presentation#1.clock_id(1)',
                 'get_xdg_surface(new id xdg_surface#2, wl_surface#3)',
                 'xdg_surface#2.get_toplevel(new id xdg_toplevel#4)']
        for i, stamp in enumerate(stamps):
            lines += [f'wp_presentation#1.feedback(wl_surface#3, new id wp_presentation_feedback#{i + 20})',
                      f'wp_presentation_feedback#{i + 20}.presented(0, {stamp // 1000000000}, {stamp % 1000000000}, 16666667, 0, {i}, 7)']
        return '\n'.join(lines)

    def test_other_surface_cannot_count(self):
        trace = self.trace([1_000_000_000]) + '\nwp_presentation#1.feedback(wl_surface#99, new id wp_presentation_feedback#999)\nwp_presentation_feedback#999.presented(0, 2, 0, 16666667, 0, 2, 7)'
        self.assertEqual(len(presentations(trace)), 1)

    def test_missing_second_counts_as_lost_refresh_slots(self):
        stamps = [i * 16_666_667 for i in range(781) if not 180 <= i < 240]
        result = analyze(self.trace(stamps), [{'commandStartMonotonicNs': 0, 'observedEndMonotonicNs': 12_000_000_000}], 60)
        self.assertLess(result['raw'][0]['withinBudgetPercent'], 91)
        self.assertGreater(result['raw'][0]['maximumGapMs'], 1000)

    def test_no_end_frame_fails_coverage(self):
        with self.assertRaisesRegex(ValueError, 'complete scroll window'):
            analyze(self.trace([1_000_000_000, 2_000_000_000]), [{'commandStartMonotonicNs': 0, 'observedEndMonotonicNs': 12_000_000_000}], 60)

    def test_realtime_clock_rejected(self):
        with self.assertRaisesRegex(ValueError, 'MONOTONIC'):
            presentations(self.trace([1]).replace('clock_id(1)', 'clock_id(0)'))


class Phrases(unittest.TestCase):
    """A content proof may only accept the document it asked for."""

    def texts(self):
        generated = Path(__file__).resolve().parent / 'generated'
        available = {name: (generated / (name + '.md')) for name in content.EXPECTED}
        if any(not path.is_file() for path in available.values()):
            self.skipTest('Fixtures not generated; run benchmarks/fixtures.py')
        # The proof only ever sees a screen, so the first part of each document.
        return {name: path.read_text()[:200000] for name, path in available.items()}

    def test_every_phrase_occurs_in_its_own_fixture(self):
        for name, text in self.texts().items():
            with self.subTest(name):
                self.assertTrue(content.contains(text, content.EXPECTED[name]))

    def test_no_phrase_set_accepts_a_different_document(self):
        texts = self.texts()
        for name, expected in content.EXPECTED.items():
            for other, text in texts.items():
                if other != name and content.contains(text, expected):
                    self.assertEqual(expected, content.EXPECTED[other],
                                     f'{name} phrases also match {other}')


class Fixtures(unittest.TestCase):
    def test_historical_manifest_detects_changed_bytes(self):
        import fixtures
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'small.md').write_text('changed')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps([{'name': 'small', 'sha256': 'original'}]))
            with patch.object(fixtures, 'MANIFEST', manifest):
                with self.assertRaisesRegex(ValueError, 'SHA-256 mismatch'):
                    verify(root)


if __name__ == '__main__':
    unittest.main()
