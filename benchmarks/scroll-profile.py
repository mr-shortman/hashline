"""Scroll windows with monotonic boundaries for a separate compositor capture.

Run under sysprof-cli --gnome-shell, with the WebDriver and CLI sharing a test bus.
"""
import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tests' / 'desktop'))
from smoke import Desktop
from environment import environment

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary')
parser.add_argument('--refresh-hz', type=float, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
if args.output.exists():
    parser.error('Output exists')
output = {'environment': environment(args.binary), 'refreshHzDeclared': args.refresh_hz,
          'method': '12 seconds continuous scrolling; monotonic command boundaries allow a 10 second interior window in an external compositor trace. rAF is diagnostic only.',
          'raw': [], 'complete': False}
app = Desktop(args.binary)
try:
    app.wait('document.querySelector(".empty-state")')
    for name in ['small', 'medium']:
        app.open(Path(f'benchmarks/generated/{name}.md').resolve())
        app.wait("document.querySelector('article')?.dataset.renderState === 'complete'", 60)
        time.sleep(2)
        for iteration in range(3):
            app.js("document.querySelector('.document-scroll').scrollTop=0")
            time.sleep(.5)
            start = time.monotonic_ns()
            app.js("""window.__scroll=null;const v=document.querySelector('.document-scroll');
              const deltas=[];let start,previous;
              function frame(now){start??=now;if(previous!==undefined)deltas.push(now-previous);previous=now;
                v.scrollTop+=8;if(now-start<12000)requestAnimationFrame(frame);
                else window.__scroll={rafIntervalsMs:deltas,elapsedMs:now-start};}
              requestAnimationFrame(frame);""")
            result = app.wait('window.__scroll', 20)
            end = time.monotonic_ns()
            output['raw'].append({'fixture': name, 'iteration': iteration,
                                  'commandStartMonotonicNs': start, 'observedEndMonotonicNs': end,
                                  'window': app.js('return {focused:document.hasFocus(),screenX,screenY,outerWidth,outerHeight,devicePixelRatio,screenWidth:screen.width,screenHeight:screen.height}'), **result})
            print(name, iteration + 1, flush=True)
    output['complete'] = True
finally:
    app.close()
    args.output.write_text(json.dumps(output, indent=2) + '\n')
