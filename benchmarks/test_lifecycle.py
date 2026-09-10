"""Run recovery, publication safety and content-resolution regression contracts."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zlib

import content
import run
import storage
from timing import Estimates, duration


class Timing(unittest.TestCase):
    def test_duration_and_persisted_cell_means(self):
        self.assertEqual(duration('60m'), 3600)
        for invalid in ('0', '-1', 'nan', '5days'):
            with self.assertRaises(ValueError):
                duration(invalid)
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'durations.json'
            estimate = Estimates(path)
            item = (0, 'content', 'small', None, ('viewmd', 'native'))
            row = dict(group='content', fixture='small', viewer='viewmd', status='ok', attempts=[{}], durationSeconds=20)
            estimate.record(row)
            self.assertEqual(estimate.seconds(item), 6)
            estimate.record(dict(row, durationSeconds=40))
            self.assertEqual(Estimates(path).seconds(item), 30)
            self.assertEqual(estimate.seconds((0, 'content', 'large', None, ('viewmd', 'native'))), 6)

    def test_setup_failure_does_not_teach_a_short_runtime(self):
        with tempfile.TemporaryDirectory() as temp:
            estimate = Estimates(Path(temp) / 'durations.json')
            row = dict(group='content', fixture='small', viewer='viewmd', status='missing', attempts=[{}],
                       durationSeconds=.1, reason='No compositor', proofFailure=None)
            estimate.record(row)
            estimate.record(row)
            self.assertEqual(estimate.seconds((0, 'content', 'small', None, ('viewmd', 'native'))), 6)


class Watchdog(unittest.TestCase):
    def test_deadline_kills_descendants_in_other_sessions(self):
        import os
        import sys
        import time
        import timing
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            # A deliberately hanging driver, with its own-session child.
            (root / 'worker.py').write_text(
                'import subprocess, sys, time\n'
                'from pathlib import Path\n'
                'p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], start_new_session=True)\n'
                'Path(sys.argv[1]).with_name("child.pid").write_text(str(p.pid))\n'
                'time.sleep(60)\n')
            with patch('timing.ROOT', root):
                result = timing.watchdog({}, root / 'artifact', .5)
            self.assertEqual(result['status'], 'timeout')
            pid = int((root / 'artifact/child.pid').read_text())
            for _ in range(20):
                try:
                    state = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
                except FileNotFoundError:
                    break
                if state == 'Z':
                    break
                time.sleep(.01)
            else:
                self.fail('watchdog left a running descendant')


class Lifecycle(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.clock = 0
        self.calls = []
        self.estimates = Estimates(self.root / 'durations.json')
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch('run.time.monotonic', side_effect=lambda: self.clock))
        self.stack.enter_context(patch('run.Estimates', return_value=self.estimates))
        self.stack.enter_context(patch.object(self.estimates, 'seconds', return_value=1))
        self.stack.enter_context(patch('run.watchdog', side_effect=self.measure))
        self.stack.enter_context(patch('session.quiet_check', return_value={'verified': True}))

    def measure(self, request, artifact, limit, heartbeat):
        self.clock += 1
        self.calls.append(request)
        heartbeat()
        return {'status': 'ok', 'metrics': {'pssMiB': 1}}

    def execute(self, *args):
        with patch('sys.argv', ['run.py', *args]), contextlib.redirect_stdout(io.StringIO()):
            return run.main()

    def new(self, *args):
        return self.execute('bench', '--only', 'memory', '--fixtures', 'small', '--renderers', 'cairo,vulkan',
                            '--repetitions', '2', '--out', str(self.root / 'run'), *args)

    def report(self):
        return json.loads((self.root / 'run/report.json').read_text())

    def test_budget_balanced_then_resume_without_repeating_rows(self):
        self.assertEqual(self.new('--time-budget', '4.1s'), 0)
        first = self.report()
        self.assertEqual(first['progress']['done'], 4)
        self.assertEqual(first['progress']['total'], 6)
        self.assertEqual(first['stoppedBy'], 'time-budget')
        self.assertEqual([r['iteration'] for r in first['rows']], [-1, -1, 0, 0])
        self.assertTrue(all(r['durationSeconds'] == 1 and r['startedAt'] for r in first['rows']))
        self.assertEqual(self.execute('--resume', str(self.root / 'run')), 0)
        resumed = self.report()
        self.assertTrue(resumed['completed'])
        self.assertEqual(resumed['rows'][:4], first['rows'])
        self.assertEqual(len(self.calls), 6)
        self.assertNotIn('stoppedBy', resumed)

    def test_partial_pass_is_completed_even_with_small_resume_budget(self):
        self.new('--time-budget', '2.1s')
        data = self.report()
        # Simulate a persisted first measurement of the next pass.
        data['rows'].append(dict(data['rows'][0], iteration=0, warmup=False))
        (self.root / 'run/report.json').write_text(json.dumps(data))
        self.execute('--resume', str(self.root / 'run'), '--time-budget', '0.1s')
        data = self.report()
        self.assertEqual(len(data['rows']), 4)
        self.assertEqual(sum(r['iteration'] == 0 for r in data['rows']), 2)
        self.assertEqual(data['stoppedBy'], 'time-budget')

    def test_only_text_failure_gets_two_retries(self):
        for status, kind, count in [('missing', 'text-not-recognized', 3),
                                    ('unrenderable', 'window-never-mapped', 1),
                                    ('missing', 'no-frames', 1)]:
            with self.subTest(status=status, kind=kind):
                directory = self.root / kind
                def result(*args):
                    self.clock += 1
                    return content.failure(kind, 'test failure', 4, status)
                with patch('run.watchdog', side_effect=result) as worker:
                    self.execute('bench', '--only', 'content', '--fixtures', 'small', '--renderers', 'cairo',
                                 '--repetitions', '1', '--out', str(directory))
                    self.assertEqual(worker.call_count, 2 * count)
                    data = json.loads((directory / 'report.json').read_text())
                    self.assertEqual(len(data['rows'][0]['attempts']), count)

    def test_active_run_is_not_overwritten(self):
        self.new('--time-budget', '2.1s')
        path = self.root / 'run'
        original = (path / 'report.json').read_bytes()
        with storage.locked(path), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.execute('--resume', str(path))
        self.assertEqual((path / 'report.json').read_bytes(), original)


class AdapterReasons(unittest.TestCase):
    def test_memory_failure_keeps_process_reason(self):
        import suite
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temp, patch('suite.isolated', return_value=contextlib.nullcontext((None, temp))), \
                patch('suite.memory_measure', return_value={'error': 'the reader exited with 7'}):
            result = suite.measure('memory', ['/missing/viewer'], Path(temp) / 'small.md', 'cairo', {},
                                   SimpleNamespace(connector='DP-3', settle=1, sample_seconds=1), Path(temp) / 'raw')
            self.assertEqual(result['status'], 'missing')
            self.assertEqual(result['reason'], 'the reader exited with 7')


class Proof(unittest.TestCase):
    def capture(self, frames, bounds=None):
        capture = object.__new__(content.Capture)
        capture.error = None
        capture.frames = [dict(receivedMonotonicNs=round(ms * 1e6), ptsNs=0,
                               packed=zlib.compress(pixels)) for ms, pixels in frames]
        if bounds:
            capture.bounds = bounds
        return capture

    def test_fallback_retains_first_mode_hits(self):
        with patch('content.ocr', side_effect=['expected']) as ocr:
            self.assertEqual(content.recognize(b'frame', ['expected'])[1:], (6, True))
            self.assertEqual(ocr.call_count, 1)
        with patch('content.ocr', side_effect=['nothing', 'expected']) as ocr:
            self.assertEqual(content.recognize(b'frame', ['expected'])[1:], (11, True))
            self.assertEqual(ocr.call_args.args, (b'frame', 11))

    def test_protected_duplicates_bound_uncertainty(self):
        capture = self.capture([(-10, b'blank'), (100, b'blank'), (110, b'blank'), (120, b'expected')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda p, psm=6: p.decode()) as ocr:
            result = capture.proof(0, ['expected'], Path(temp))
            self.assertEqual(result['readableLowerMs'], 110)
            self.assertEqual(result['readableUpperMs'], 120)
            self.assertEqual(result['proofGapMs'], 10)
            self.assertEqual(len(result['frames']), 4)
            self.assertTrue(result['proofResolutionValid'])
            self.assertEqual(ocr.call_count, 3)  # one negative in two modes, one hit
            replay = content.replay(Path(temp) / 'capture.json', Path(temp) / 'replay')
            self.assertEqual(replay['proofGapMs'], 10)

    def test_one_skipped_refresh_still_resolves(self):
        # The capture delivers at best one frame per refresh and skips to every
        # second refresh under load; measured spacing reached 36.57 ms. A bound
        # that rejects that rejects a third of sound proofs for jitter alone.
        for gap in (33.35, 36.57):
            with self.subTest(gap=gap):
                capture = self.capture([(-10, b'blank'), (0, b'blank'), (gap, b'expected')])
                with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda p, psm=6: p.decode()):
                    result = capture.proof(0, ['expected'], Path(temp))
                    self.assertEqual(result['status'], 'ok')
                    self.assertTrue(result['proofResolutionValid'])
        # Three skipped refreshes remain unresolved, so the guard still bites.
        capture = self.capture([(-10, b'blank'), (0, b'blank'), (50, b'expected')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda p, psm=6: p.decode()):
            self.assertEqual(capture.proof(0, ['expected'], Path(temp))['status'], 'diagnostic')

    def test_400ms_gap_never_passes(self):
        capture = self.capture([(-10, b'blank'), (390, b'expected')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda p, psm=6: p.decode()):
            result = capture.proof(0, ['expected'], Path(temp))
            self.assertEqual(result['status'], 'diagnostic')
            self.assertFalse(result['proofResolutionValid'])
            self.assertEqual(result['proofGapMs'], 400)

    def test_dedup_and_ocr_use_only_window_pixels(self):
        # Outside-window changes cannot generate extra OCR or accepted words.
        ppm = lambda inside, outside: b'P6\n2 1\n255\n' + inside + outside
        capture = self.capture([(-1000, ppm(b'abc', b'123')), (-600, ppm(b'abc', b'456')),
                                (0, ppm(b'abc', b'789')), (10, ppm(b'def', b'789'))],
                               dict(x=0, y=0, width=1, height=1))
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda p, psm=6: 'expected' if p.endswith(b'def') else '') as ocr:
            result = capture.proof(0, ['expected'], Path(temp))
            self.assertEqual(result['retainedFrames'], 3)
            self.assertTrue(all(call.args[0].startswith(b'P6\n1 1\n255\n') for call in ocr.call_args_list))
            self.assertEqual((Path(temp) / 'first-readable.ppm').read_bytes(), b'P6\n1 1\n255\ndef')

    def test_slow_response_uses_last_duplicate_as_lower_bound(self):
        capture = self.capture([(-10, b'blank'), (600, b'blank'), (1600, b'blank'), (1610, b'expected')])
        with tempfile.TemporaryDirectory() as temp, patch('content.ocr', side_effect=lambda p, psm=6: p.decode()):
            result = capture.proof(0, ['expected'], Path(temp))
            self.assertEqual(result['readableLowerMs'], 1600)
            self.assertEqual(result['proofGapMs'], 10)

    def test_no_frames_is_structured(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self.capture([]).proof(0, ['expected'], Path(temp))
            self.assertEqual(result['proofFailure']['kind'], 'no-frames')


class Publication(unittest.TestCase):
    def test_guard_rejects_raw_and_oversize(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'run/raw').mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, 'raw'):
                storage.check_results(root)
            (root / 'run/raw').rmdir()
            (root / 'run/large').write_bytes(b'abcdef')
            with patch('storage.MAX_BYTES', 5), self.assertRaisesRegex(ValueError, 'exceeds'):
                storage.check_results(root)

    def test_promote_has_one_png_per_cell_and_full_manifest(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / 'local'; source.mkdir()
            artifact = source / 'raw'; artifact.mkdir()
            proof = artifact / 'frame.ppm'; proof.write_bytes(b'P6\n1 1\n255\nabc')
            row = dict(viewer='hashline', renderer='cairo', group='content', fixture='small', refreshHz=None,
                       status='ok', warmup=False, evidence=str(proof), frames=[{'x': 1}], raw={'large': 'value'})
            output = dict(rows=[row, row], summary=[], mode='bench', repetitions={'content': 2},
                          completed=True, acceptance=False, viewers={'hashline': {'role': 'self'}})
            (source / 'report.json').write_text(json.dumps(output))
            with patch('storage.RESULTS', root / 'published'), contextlib.redirect_stdout(io.StringIO()):
                target = storage.promote(source)
            report = json.loads((target / 'report.json').read_text())
            self.assertEqual(len(list((target / 'proofs').glob('*.png'))), 1)
            self.assertNotIn('frames', report['rows'][0])
            self.assertNotIn('raw', report['rows'][0])
            manifest = json.loads((target / 'MANIFEST.json').read_text())
            self.assertEqual({f['path'] for f in manifest['files']}, {'raw/frame.ppm', 'report.json'})
            self.assertTrue(all(f.get('sha256') for f in manifest['files']))
            storage.check_results(root / 'published')

    def test_force_never_disables_repository_guard(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / 'local'; source.mkdir()
            output = dict(rows=[], summary=[], mode='bench', repetitions={}, completed=False,
                          acceptance=False, viewers={})
            (source / 'report.json').write_text(json.dumps(output))
            with patch('storage.RESULTS', root / 'published'), patch('storage.MAX_BYTES', 1), contextlib.redirect_stdout(io.StringIO()):
                target = storage.promote(source, force=True)
                with self.assertRaisesRegex(ValueError, 'exceeds'):
                    storage.check_results(target.parent)

    def test_empty_and_unexplained_partial_runs_refused_with_size(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / 'local'; source.mkdir()
            output = dict(rows=[], summary=[], mode='bench', repetitions={}, completed=False,
                          acceptance=False, viewers={})
            (source / 'report.json').write_text(json.dumps(output))
            printed = io.StringIO()
            with patch('storage.RESULTS', root / 'published'), contextlib.redirect_stdout(printed):
                with self.assertRaisesRegex(ValueError, 'empty'):
                    storage.promote(source)
            self.assertIn('bytes', printed.getvalue())
            self.assertFalse((root / 'published/local').exists())


if __name__ == '__main__':
    unittest.main()
