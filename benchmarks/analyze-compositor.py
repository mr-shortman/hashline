"""Extract monitor presentation bounds from sysprof-cat marks, not app frames.

Dump a saved capture with sysprof-cat --no-callgraph --no-counters first.
Mutter records the time since presentation from *inside* a scoped trace mark.
The presentation timestamp is therefore bounded by (mark start/end - delay).
See GNOME/mutter 50.4 clutter/clutter/clutter-frame-clock.c, notify_presented.
The interval classification retains this uncertainty instead of rounding it away.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dump', type=Path)
    parser.add_argument('--scroll', type=Path, required=True)
    parser.add_argument('--display', type=Path, required=True)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    from storage import require_local_output
    if args.output is not None:
        try:
            require_local_output(args.output)
        except ValueError as error:
            parser.error(str(error))
    if args.output.exists():
        parser.error('Output exists')
    display = json.loads(args.display.read_text())
    scroll = json.loads(args.scroll.read_text())
    connector = display['primaryConnector']
    marks = []
    pattern = re.compile(r'name: "Clutter::FrameClock::presented\(\)";\s+message: "([^"\n]+)";\s+duration: (\d+);\s+end-time: (\d+);')
    for message, duration, end in pattern.findall(args.dump.read_text()):
        if not message.startswith(connector + ','):
            continue
        delay = re.search(r'presentation (was|will be) (\d+) µs (earlier|later)', message)
        if not delay:
            continue
        shift = int(delay[2]) * 1000 * (-1 if delay[1] == 'was' else 1)
        marks.append({'traceEndNs': int(end), 'traceDurationNs': int(duration),
                      'message': message, 'lowerNs': int(end) - int(duration) + shift,
                      'upperNs': int(end) + shift})
    marks.sort(key=lambda row: row['lowerNs'])
    if not marks:
        parser.error('No recognized presentation marks for selected connector')
    budget = 16.7 if scroll['refreshHzDeclared'] < 100 else 8.37
    rows = []
    for sample in scroll['raw']:
        start = sample['commandStartMonotonicNs'] + 1_000_000_000
        end = start + 10_000_000_000
        if end >= sample['observedEndMonotonicNs']:
            parser.error('Scroll window too short for 10-second interior')
        intervals = []
        for previous, current in zip(marks, marks[1:]):
            if current['upperNs'] <= start or previous['lowerNs'] >= end:
                continue
            intervals.append({'minimumMs': max(0, current['lowerNs'] - previous['upperNs']) / 1e6,
                              'maximumMs': max(0, current['upperNs'] - previous['lowerNs']) / 1e6})
        rows.append({'fixture': sample['fixture'], 'iteration': sample['iteration'],
                     'startMonotonicNs': start, 'endMonotonicNs': end,
                     'intervals': intervals, 'n': len(intervals),
                     'definitelyWithinBudgetPercent': 100 * sum(i['maximumMs'] <= budget for i in intervals) / len(intervals) if intervals else None,
                     'possiblyWithinBudgetPercent': 100 * sum(i['minimumMs'] <= budget for i in intervals) / len(intervals) if intervals else None,
                     'maximumGapUpperBoundMs': max((i['maximumMs'] for i in intervals), default=None)})
    output = {'capture': str(args.capture), 'captureSha256': hashlib.sha256(args.capture.read_bytes()).hexdigest(),
              'source': 'https://github.com/GNOME/mutter/blob/50.4/clutter/clutter/clutter-frame-clock.c',
              'method': 'Monitor-wide presentation-time bounds inferred from scoped mark and presentation delay; interval pairs overlap each 10-second interior window. Includes all clients; no app surface/frame identifiers. Not an application presentation acceptance.',
              'applicationFramesVerified': False, 'connector': connector, 'budgetMs': budget,
              'marks': marks, 'raw': rows}
    args.output.write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps([{k:v for k,v in row.items() if k != 'intervals'} for row in rows], indent=2))


if __name__ == '__main__':
    main()
