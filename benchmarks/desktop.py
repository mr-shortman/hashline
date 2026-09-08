"""WebKit helper timings, NOT externally verified presentation/launch timings.

Start tauri-driver; run python3 benchmarks/desktop.py /path/to/hashline [repetitions].
"""
import json
from pathlib import Path
import shutil
import statistics
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tests' / 'desktop'))
from smoke import Desktop  # noqa: E402

repetitions = int(sys.argv[2]) if len(sys.argv) > 2 else 30
results = []
with tempfile.TemporaryDirectory(prefix='hashline-benchmark-') as directory:
    root = Path(directory)
    app = Desktop(sys.argv[1])
    try:
        app.wait('document.querySelector(".empty-state")')
        for name in ['small', 'medium', 'large']:
            for suffix in ['a', 'b']:
                shutil.copy(f'benchmarks/generated/{name}.md', root / f'{name}-{suffix}.md')
            for i in range(repetitions if name != 'large' else 1):
                app.js('performance.clearMeasures()')
                start = time.monotonic()
                app.open(root / f'{name}-{"a" if i % 2 == 0 else "b"}.md')
                app.wait("performance.getEntriesByName('hashline.content-to-frame').length > 0", timeout=60)
                marks = app.js("return Object.fromEntries(performance.getEntriesByType('measure').map(m=>[m.name,m.duration]))")
                results.append({'fixture': name, 'iteration': i, 'cliToObservedFrameMs': (time.monotonic() - start) * 1000, **marks})
                print(name, i + 1, round(results[-1]['cliToObservedFrameMs']), flush=True)
                time.sleep(.1)
    finally:
        app.close()
        output = {'method': 'Release app through WebKitGTK WebDriver. CLI-to-observed-frame includes subprocess and driver overhead; double requestAnimationFrame is only a helper. Not a compositor or installed-package measurement.', 'repetitions': repetitions, 'raw': results, 'summary': {}}
        for name in ['small', 'medium', 'large']:
            rows = [r for r in results if r['fixture'] == name]
            if not rows:
                continue
            output['summary'][name] = {key: {'median': statistics.median(r[key] for r in rows), 'p95': sorted(r[key] for r in rows)[max(0, int(len(rows) * .95 + .999) - 1)]} for key in rows[0] if key not in ['fixture', 'iteration']}
        Path('benchmarks/results').mkdir(exist_ok=True)
        Path('benchmarks/results/webkit.json').write_text(json.dumps(output, indent=2) + '\n')
