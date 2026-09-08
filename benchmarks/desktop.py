"""Release WebKitGTK measurements. All frame times are explicitly JS helpers.

python3 benchmarks/desktop.py /absolute/hashline --mode open --repetitions 30
Use distinct --output paths for comparable builds. Never overwrites legacy data.
"""
import argparse
import hashlib
import json
import math
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
from processes import ImageMemory, app_pid  # noqa: E402


def summary(rows):
    groups = {}
    for row in rows:
        groups.setdefault(row['fixture'], []).append(row)
    return {name: {key: {'n': len(values), 'median': statistics.median(values),
                         'p95': sorted(values)[math.ceil(len(values) * .95) - 1],
                         'max': max(values), 'regularSeries': len(values) >= 30}
                   for key in sorted({k for row in group for k, v in row.items()
                                      if isinstance(v, (int, float)) and k != 'iteration'})
                   if (values := [r[key] for r in group if isinstance(r.get(key), (int, float))])}
            for name, group in groups.items()}


def complete(app):
    app.wait("document.querySelector('article')?.dataset.renderState === 'complete'", 180)


def marks(app):
    return app.js("return Object.fromEntries(performance.getEntriesByType('measure').map(m=>[m.name,m.duration]))")


def timed_ui(app, action, condition):
    # The clock starts in the event's task. Driver polling does not enter this duration.
    app.js("""window.__benchResult=null;
      const start=performance.now(); let frames=0;
      const check=()=>{if(CONDITION) requestAnimationFrame(()=>requestAnimationFrame(()=>{
        window.__benchResult={eventToFrameHelperMs:performance.now()-start};
      }));else if(++frames<600)requestAnimationFrame(check);
      else window.__benchResult={error:'UI condition timeout'};};
      ACTION; requestAnimationFrame(check);
    """.replace('ACTION', action).replace('CONDITION', condition))
    result = app.wait('window.__benchResult', 20)
    if 'error' in result:
        raise RuntimeError(result['error'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('--mode', choices=['open', 'startup', 'interaction', 'scroll', 'images'], default='open')
    parser.add_argument('--repetitions', type=int, default=30)
    parser.add_argument('--fixtures', nargs='+', help='Fixture names; defaults depend on measurement mode')
    parser.add_argument('--distinct-content', action='store_true', help='Alternate changed headings throughout the document to prevent reuse-only comparisons')
    parser.add_argument('--refresh-hz', type=float, help='Verified physical refresh rate; still no compositor proof')
    parser.add_argument('--output', type=Path, default=Path('benchmarks/results/sections-open.json'))
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error('--repetitions must be positive')
    if args.output.exists():
        parser.error('Output already exists; choose a new path to preserve provenance')
    output = {'schemaVersion': 2, 'mode': args.mode, 'environment': environment(args.binary),
              'method': 'Release WebKitGTK WebDriver. JS double-rAF is a layout/frame helper, NOT actual presentation. CLI/session observations include driver costs. OS file cache uncontrolled. No reference acceptance without external presentation/compositor evidence.',
              'physicalRefreshHzDeclared': args.refresh_hz, 'repetitions': args.repetitions,
              'distinctContent': args.distinct_content,
              'startedAtUtc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'fixtures': {}, 'raw': [], 'completed': False}
    def checkpoint():
        # Persist each completed observation; even SIGKILL must not erase a long series.
        output['summary'] = summary(output['raw'])
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix(args.output.suffix + '.tmp')
        temporary.write_text(json.dumps(output, indent=2) + '\n')
        temporary.replace(args.output)

    checkpoint()
    app = None
    image_memory = None
    try:
        with tempfile.TemporaryDirectory(prefix='hashline-benchmark-') as directory:
            root = Path(directory)
            names = args.fixtures or (['small', 'medium', 'large'] if args.mode == 'open' else (['many-images', 'large-images'] if args.mode == 'images' else ['small', 'medium']))
            if args.mode == 'startup':
                names = ['small']
            for name in names:
                source = Path(f'benchmarks/generated/{name}.md')
                output['fixtures'][name] = {'bytes': source.stat().st_size,
                    'sha256': hashlib.sha256(source.read_bytes()).hexdigest()}
                for suffix in ['a', 'b']:
                    target = root / f'{name}-{suffix}.md'
                    if args.distinct_content and suffix == 'b':
                        target.write_text(source.read_text().replace('Abschnitt', 'AbschnitX'))
                    else:
                        shutil.copy(source, target)
                    output['fixtures'][name][suffix + 'Sha256'] = hashlib.sha256(target.read_bytes()).hexdigest()
            for image in Path('benchmarks/generated').glob('*.png'):
                shutil.copy(image, root)
            if args.mode != 'startup':
                app = Desktop(args.binary)
                app.wait('document.querySelector(".empty-state")')
            for name in names:
                if args.mode in ['interaction', 'scroll']:
                    app.open(root / f'{name}-a.md')
                    complete(app)
                    time.sleep(1.5)
                for i in range(args.repetitions):
                    path = root / f'{name}-{"a" if i % 2 == 0 else "b"}.md'
                    row = {'fixture': name, 'iteration': i}
                    if args.mode in ['open', 'startup', 'images']:
                        if args.mode == 'images':
                            image_memory = ImageMemory(args.binary)
                        if app:
                            app.js("""performance.clearMeasures();
                              window.__loopDelays=[];let previous=performance.now();
                              clearInterval(window.__loopTimer);
                              window.__loopTimer=setInterval(()=>{const now=performance.now();
                                window.__loopDelays.push(Math.max(0,now-previous-16));previous=now;},16);""")
                        start = time.monotonic()
                        if args.mode == 'startup':
                            app = Desktop(args.binary, [str(path)])
                        else:
                            app.open(path)
                        app.wait("performance.getEntriesByName('hashline.content-to-frame').length > 0", 60)
                        row['driverToObservedFirstFrameMs'] = (time.monotonic() - start) * 1000
                        complete(app)
                        row.update(marks(app))
                        row['reusedSections'] = app.js("return Number(document.querySelector('article')?.dataset.reusedSections || 0)")
                        row['renderTasks'] = app.js("return Number(document.querySelector('article')?.dataset.renderTasks || 0)")
                        if args.mode != 'startup':
                            delays = app.js('clearInterval(window.__loopTimer);return window.__loopDelays')
                            row['eventLoopDelaysMs'] = delays
                            row['maxEventLoopDelayMs'] = max(delays, default=0)
                        if args.mode == 'images':
                            # Exercise all lazy resources, including decode dimensions.
                            count = app.js("return document.querySelectorAll('article img').length")
                            for index in range(count):
                                app.js("document.querySelectorAll('article img')[arguments[0]].scrollIntoView()", index)
                                app.wait(f"document.querySelectorAll('article img')[{index}].complete", 30)
                            row['imageLoadAndScrollMs'] = (time.monotonic() - start) * 1000
                            row['images'] = app.js("return Array.from(document.querySelectorAll('article img'),i=>({width:i.naturalWidth,height:i.naturalHeight,unavailable:i.hasAttribute('data-unavailable')}))")
                        if args.mode == 'images':
                            row['memory'] = image_memory.finish()
                            image_memory = None
                        if args.mode == 'startup':
                            app.close()
                            app = None
                    elif args.mode == 'interaction':
                        for label, action, condition in [
                            ('searchOpen', "document.querySelector('[aria-label=Suche]').click()", "document.querySelector('.search-bar input')===document.activeElement"),
                            ('menuOpen', "document.querySelector('[aria-label=\"Darstellung und Optionen\"]').click()", "document.querySelector('.settings')"),
                            ('menuClose', "document.querySelector('[aria-label=\"Darstellung und Optionen\"]').click()", "!document.querySelector('.settings')"),
                        ]:
                            row[label + 'ToFrameHelperMs'] = timed_ui(app, action, condition)['eventToFrameHelperMs']
                        app.js('performance.clearMeasures()')
                        query = 'Suchnadel' if i % 2 == 0 else 'Abschnitt'
                        row['searchResultsToFrameHelperMs'] = timed_ui(app,
                            "const input=document.querySelector('.search-bar input'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input," + json.dumps(query) + "); input.dispatchEvent(new Event('input',{bubbles:true}));",
                            "performance.getEntriesByName('hashline.search').length>0 && document.querySelector('.search-count').textContent.includes(' / ')")['eventToFrameHelperMs']
                        row.update(marks(app))
                        app.js("document.querySelector('[aria-label=\"Suche schließen\"]').click()")
                    else:
                        app.js("""window.__benchResult=null; const v=document.querySelector('.document-scroll');
                          v.scrollTop=0; const deltas=[]; let start,previous;
                          function tick(now){start??=now; if(previous!==undefined)deltas.push(now-previous);
                            previous=now; v.scrollTop+=8;
                            if(now-start<10000)requestAnimationFrame(tick);
                            else window.__benchResult={rafIntervalsMs:deltas,elapsedMs:now-start};}
                          requestAnimationFrame(tick);""")
                        row.update(app.wait('window.__benchResult', 20))
                        intervals = row['rafIntervalsMs']
                        budget = 1000 / args.refresh_hz + .04 if args.refresh_hz else 16.7
                        row['rafWithinBudgetPercent'] = 100 * sum(t <= budget for t in intervals) / len(intervals)
                        row['rafMaxGapMs'] = max(intervals)
                    output['raw'].append(row)
                    checkpoint()
                    print(args.mode, name, i + 1, flush=True)
            if args.mode == 'images':
                small = root / 'return-small.md'
                shutil.copy('benchmarks/generated/small.md', small)
                app.open(small)
                complete(app)
                time.sleep(3)
                output['smallAfterImages'] = json.loads(subprocess.check_output([
                    sys.executable, 'benchmarks/process-sample.py', str(app_pid(args.binary)), '30']))
            output['completed'] = True
    except Exception as error:
        output['error'] = str(error)
        raise
    finally:
        if image_memory:
            output['interruptedImageMemory'] = image_memory.finish()
        if app:
            try:
                app.close()
            except Exception as error:
                output['cleanupError'] = str(error)
        checkpoint()


if __name__ == '__main__':
    main()
