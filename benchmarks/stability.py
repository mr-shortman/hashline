"""Settled release app PSS/CPU, including completed renders on all 50 switches."""
import argparse
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tests' / 'desktop'))
from smoke import Desktop  # noqa: E402
from environment import environment  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('--output', type=Path, default=Path('benchmarks/results/sections-stability.json'))
    args = parser.parse_args()
    if args.output.exists():
        parser.error('Output exists; use a new path')
    output = {'environment': environment(args.binary), 'warmupSwitches': 10, 'switches': 50, 'completed': False}
    with tempfile.TemporaryDirectory(prefix='hashline-stability-') as directory:
        paths = [Path(directory) / f'small-{name}.md' for name in ['a', 'b']]
        for path in paths:
            shutil.copy('benchmarks/generated/small.md', path)
        app = Desktop(args.binary, [str(paths[0])])
        def wait():
            app.wait("document.querySelector('article')?.dataset.renderState === 'complete'", 60)
        def switch(i):
            app.open(paths[(i + 1) % 2])
            wait()
        def sample():
            time.sleep(3)  # Highlighter, worker retirement and queued layouts settle.
            pids = [int(p.name) for p in Path('/proc').iterdir() if p.name.isdigit() and os.path.realpath(p / 'exe') == app.binary]
            if len(pids) != 1:
                raise RuntimeError(f'Expected one app process, found {pids}')
            return json.loads(subprocess.check_output([sys.executable, 'benchmarks/process-sample.py', str(pids[0]), '30']))
        try:
            wait()
            output['initial'] = sample()
            for i in range(10):
                switch(i)
            print('Sampling warmed baseline', flush=True)
            output['baseline'] = sample()
            for i in range(50):
                switch(i)
            print('Sampling after 50 completed switches', flush=True)
            output['after50Switches'] = sample()
            def pss(key):
                samples = output[key]['samples']
                values = [s['pssKiB'] for s in samples if s['pssKiB'] is not None]
                return statistics.median(values) if len(values) == len(samples) else None
            baseline, after = pss('baseline'), pss('after50Switches')
            output['summary'] = {'initialMedianPssMiB': pss('initial') / 1024 if pss('initial') is not None else None,
                                 'baselineMedianPssMiB': baseline / 1024 if baseline is not None else None,
                                 'afterMedianPssMiB': after / 1024 if after is not None else None,
                                 'growthPercent': (after / baseline - 1) * 100 if baseline and after is not None else None}
            output['completed'] = True
            print(json.dumps(output['summary']), flush=True)
        except Exception as error:
            output['error'] = str(error)
            raise
        finally:
            app.close()
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(output, indent=2) + '\n')


if __name__ == '__main__':
    main()
