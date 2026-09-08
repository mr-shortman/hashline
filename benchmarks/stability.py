"""App-process PSS/idle CPU before and after 50 real native file switches."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tests' / 'desktop'))
from smoke import Desktop  # noqa: E402

with tempfile.TemporaryDirectory(prefix='hashline-stability-') as directory:
    paths = [Path(directory) / f'small-{name}.md' for name in ['a', 'b']]
    for path in paths:
        shutil.copy('benchmarks/generated/small.md', path)
    app = Desktop(sys.argv[1], [str(paths[0])])
    try:
        app.wait('document.querySelector("article h1")')
        for i in range(10):
            app.open(paths[(i + 1) % 2])
        time.sleep(3)
        pid = next(int(p.name) for p in Path('/proc').iterdir() if p.name.isdigit() and os.path.realpath(p / 'exe') == app.binary)
        print('Sampling baseline', flush=True)
        baseline = json.loads(subprocess.check_output([sys.executable, 'benchmarks/process-sample.py', str(pid), '30']))
        for i in range(50):
            app.open(paths[(i + 1) % 2])
        time.sleep(3)
        print('Sampling after 50 switches', flush=True)
        after = json.loads(subprocess.check_output([sys.executable, 'benchmarks/process-sample.py', str(pid), '30']))
        output = {'warmupSwitches': 10, 'baseline': baseline, 'after50Switches': after}
        Path('benchmarks/results/stability.json').write_text(json.dumps(output, indent=2) + '\n')
        print(json.dumps({'baselineLastPssKiB': baseline['samples'][-1]['pssKiB'], 'afterLastPssKiB': after['samples'][-1]['pssKiB'], 'baselineCpuPercent': baseline['cpuPercentOfOneCore'], 'afterCpuPercent': after['cpuPercentOfOneCore']}), flush=True)
    finally:
        app.close()
