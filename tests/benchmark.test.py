"""Run with python3 tests/benchmark.test.py (no third-party Python dependencies)."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'benchmarks'))
from desktop import summary


class BenchmarkContracts(unittest.TestCase):
    def test_nearest_rank_p95_and_incomplete_series(self):
        result = summary([{'fixture': 'medium', 'iteration': i, 'ms': i + 1} for i in range(30)])['medium']['ms']
        self.assertEqual(result['median'], 15.5)
        self.assertEqual(result['p95'], 29)
        self.assertTrue(result['regularSeries'])
        self.assertFalse(summary([{'fixture': 'large', 'ms': 1}])['large']['ms']['regularSeries'])

    def test_missing_samples_are_not_filled_with_zero(self):
        result = summary([{'fixture': 'x', 'ms': 10}, {'fixture': 'x', 'otherMs': 5}])['x']
        self.assertEqual(result['ms']['n'], 1)
        self.assertEqual(result['ms']['median'], 10)

    def test_exited_root_is_never_valid_zero_memory_or_cpu(self):
        result = json.loads(subprocess.check_output([
            sys.executable, 'benchmarks/process-sample.py', '2147483647', '1']))
        self.assertFalse(result['stableProcessGroup'])
        self.assertIsNone(result['cpuPercentOfOneCore'])
        self.assertTrue(all(s['pssKiB'] is None for s in result['samples']))
        self.assertGreaterEqual(result['samples'][-1]['seconds'] - result['samples'][0]['seconds'], .99)


if __name__ == '__main__':
    unittest.main()
