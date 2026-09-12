#!/usr/bin/env python3
"""One reproducible bench/compare matrix with interleaving and fail-closed budgets."""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time
import signal

from timing import Estimates, duration, human, identity, row_identity, utcnow, watchdog
from storage import RUNS, RESULTS, locked, resolve_run

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
from content import engine
from fixtures import MANIFEST, ensure
from provision import LOCAL, catalog, locate, provision, tool

GROUPS = ('stages', 'startup', 'content', 'memory', 'idle', 'scroll', 'interaction', 'tabs', 'reload', 'stability', 'mainthread')
INTERNAL = {'stages', 'interaction', 'stability', 'mainthread'}
SCREEN_GROUPS = {'content', 'interaction', 'tabs', 'reload', 'scroll'}
SIZE = {'small': 0, 'medium': 1, 'large': 2}
# Repetitions an acceptance needs. A p95 needs a long series; the idle target is
# a single continuous 30-second window, and repeating it thirty times only
# repeats the waiting. Every group must still be complete.
REQUIRED = {'idle': 5}
SERIES = 30
# Every decision-014 target is represented, including targets without an observer.
BUDGETS = {
    # Only the content proof measures "start to readable text"; `startup` reports
    # the first scanned-out frame, which is evidence but not this target.
    'readableUpperMs': ('content', '<=', [120, 150, 300], 'p95'),
    'openExistingMs': ('interaction', '<=', [40, 80, 250], 'p95'),
    'pssMiB': ('memory', '<=', [40, 70, 150], 'max'),
    'memoryGrowthMiB': ('memory', '<=', None, 'max'),
    'inactiveTabMiB': ('tabs', '<=', None, 'max'),
    'tenTabsMiB': ('tabs', '<=', 110, 'max'),
    'idleCpuPercent': ('idle', '<', .3, 'max'),
    'scrollWithinPercent': ('scroll', '>=', 99, 'min'),
    'scrollMaxGapMs': ('scroll', '<=', 33, 'max'),
    'mainThreadMaxMs': ('interaction/mainthread', '<=', 16, 'max'),
    'searchLargeMs': ('interaction', '<=', 120, 'p95'),
    'searchOpenUpperMs': ('interaction', '<=', 25, 'p95'),
    'menuOpenMs': ('interaction', '<=', 25, 'p95'),
    'tabSwitchUpperMs': ('tabs', '<=', 30, 'p95'),
    'reloadUpperMs': ('reload', '<=', 250, 'p95'),
    'reloadAnchorPreserved': ('reload', '==', True, 'min'),
    'installedMiB': ('package', '<=', 20, 'max'),
    'stabilityGrowthPercent': ('stability', '<=', 20, 'max'),
}


def selected(value, valid):
    names = value.split(',') if value else list(valid)
    if not names or any(name not in valid for name in names):
        raise argparse.ArgumentTypeError(f'Choose comma-separated names from: {", ".join(valid)}')
    return list(dict.fromkeys(names))


def quantiles(values):
    ordered = sorted(values)
    return {'n': len(values), 'median': statistics.median(values),
            'p95': ordered[math.ceil(.95 * len(ordered)) - 1], 'min': ordered[0], 'max': ordered[-1]}


def counts(groups, repetitions):
    """Repetitions per group, from one number or an explicit mapping."""
    if isinstance(repetitions, dict):
        return {group: repetitions[group] for group in groups}
    return {group: min(repetitions, REQUIRED.get(group, repetitions)) for group in groups}


def plan_repetitions(value, groups, default, idle_default):
    """`30`, `content=30,memory=8`, or a mix — a bare number moves the default.

    An hour is a fixed budget, and the groups do not deserve equal shares of it.
    A p95 needs a long series; a settled PSS is flat within a second and reads
    the same on the eighth pass as on the thirtieth. Spending the budget where
    the variance is beats one number for everything.
    """
    overrides = {}
    for item in (value.split(',') if value else []):
        name, separator, count = item.partition('=')
        if not separator:
            name, count = None, item
        try:
            number = int(count)
        except ValueError:
            raise argparse.ArgumentTypeError(f'Not a repetition count: {item!r}')
        if number < 1:
            raise argparse.ArgumentTypeError(f'Repetitions must be positive: {item!r}')
        if name is None:
            default = number
        elif name not in GROUPS:
            raise argparse.ArgumentTypeError(f'Unknown group {name!r}; choose from: {", ".join(GROUPS)}')
        else:
            overrides[name] = number
    planned = {group: overrides.get(group, default) for group in groups}
    # The idle window is 30 s of waiting; without an explicit budget it keeps
    # the small series the target asks for rather than the run-wide default.
    if 'idle' in planned and 'idle' not in overrides:
        planned['idle'] = min(planned['idle'], idle_default)
    return planned


def schedule(viewers, groups, fixtures, repetitions, warmup=True, refresh_hz=None):
    planned = counts(groups, repetitions)
    for iteration in range(-1 if warmup else 0, max(planned.values())):
        offset = (iteration + 1) % len(viewers)
        order = viewers[offset:] + viewers[:offset]
        if iteration % 2:
            order = list(reversed(order))
        for group in groups:
            if iteration >= planned[group]:
                continue
            for fixture in fixtures:
                for hz in [refresh_hz]:  # One condition for the entire run, never a mode sweep.
                    for viewer in order:
                        yield iteration, group, fixture, hz, viewer


def summarize(rows, repetitions):
    planned = counts({row['group'] for row in rows}, repetitions)
    cells = {}
    for row in rows:
        if row['warmup']:
            continue
        key = (row['viewer'], row['renderer'], row['group'], row['fixture'], row['refreshHz'])
        cells.setdefault(key, []).append(row)
    result = []
    for (viewer, renderer, group, fixture, hz), samples in cells.items():
        expected = planned[group]
        metrics = {}
        for name in sorted({name for row in samples for name in row.get('metrics', {})}):
            values = [row['metrics'][name] for row in samples if row.get('metrics', {}).get(name) is not None]
            if values:
                metrics[name] = quantiles(values)
        complete = len(samples) == expected and all(row['status'] == 'ok' for row in samples)
        budgets = []
        for name, (groups, operator, limits, statistic) in BUDGETS.items():
            if group not in groups.split('/'):
                continue
            if name == 'memoryGrowthMiB' and fixture != 'large':
                continue
            if name == 'searchLargeMs' and fixture != 'large':
                continue
            # Ten tabs of a megabyte each is one number, and only the medium
            # fixture is a megabyte.
            if name == 'tenTabsMiB' and fixture != 'medium':
                continue
            if isinstance(limits, list) and fixture not in SIZE:
                continue
            limit = limits[SIZE[fixture]] if isinstance(limits, list) and fixture in SIZE else limits
            if isinstance(limit, list):
                limit = None
            if name == 'inactiveTabMiB':
                limit = 3 * (ROOT / 'generated' / (fixture + '.md')).stat().st_size / 1048576
            value = metrics.get(name, {}).get(statistic)
            known = value is not None and limit is not None and complete and metrics[name]['n'] == expected
            if any(r.get('sessionKind') == 'nested-headless' for r in samples) and name in ('scrollWithinPercent', 'scrollMaxGapMs', 'presentedMs'):
                known = False
            if group in SCREEN_GROUPS and any(r.get('proofResolutionValid') is False for r in samples):
                known = False
            passed = known and {'<=': lambda: value <= limit, '<': lambda: value < limit,
                               '>=': lambda: value >= limit, '==': lambda: value == limit}[operator]()
            budgets.append({'metric': name, 'operator': operator, 'limit': limit,
                            'statistic': statistic, 'value': value,
                            'status': ('pass' if passed else 'fail') if known else 'unmeasured'})
        result.append({'viewer': viewer, 'renderer': renderer, 'group': group, 'fixture': fixture,
                       'refreshHz': hz, 'n': len(samples), 'complete': complete, 'budgeted': bool(budgets),
                       'required': REQUIRED.get(group, SERIES), 'sufficient': expected >= REQUIRED.get(group, SERIES),
                       'acceptance': complete and expected >= REQUIRED.get(group, SERIES) and bool(budgets)
                                     and all(b['status'] == 'pass' for b in budgets),
                       'statuses': {status: sum(r['status'] == status for r in samples) for status in sorted({r['status'] for r in samples})},
                       'metrics': metrics, 'budgets': budgets})
    # Growth is paired by run, so it cannot subtract minima from unrelated runs.
    for cell in result:
        if cell['group'] != 'memory' or cell['fixture'] != 'large':
            continue
        small = {row['iteration']: row for row in rows if not row['warmup'] and row['group'] == 'memory'
                 and row['fixture'] == 'small' and row['viewer'] == cell['viewer'] and row['renderer'] == cell['renderer']}
        large = [row for row in rows if not row['warmup'] and row['group'] == 'memory'
                 and row['fixture'] == 'large' and row['viewer'] == cell['viewer'] and row['renderer'] == cell['renderer']]
        values = [row['metrics']['pssMiB'] - small[row['iteration']]['metrics']['pssMiB'] for row in large
                  if row['status'] == 'ok' and row['iteration'] in small and small[row['iteration']]['status'] == 'ok']
        if len(values) == planned['memory'] and cell['complete']:
            limit = 2 * (ROOT / 'generated/large.md').stat().st_size / 1048576
            budget = next(b for b in cell['budgets'] if b['metric'] == 'memoryGrowthMiB')
            budget.update(limit=limit, value=max(values), status='pass' if max(values) <= limit else 'fail')
    return result


def accepted(summary, eligible):
    """A group without a target — stages, startup — must still be complete, but
    cannot carry an acceptance, and a run of only such groups is never one."""
    return bool(eligible and any(cell['budgeted'] for cell in summary)
                and all(cell['sufficient'] for cell in summary)
                and all(cell['acceptance'] for cell in summary if cell['budgeted']))


def report(output):
    counted = '; '.join(f'{group} n={n}' for group, n in sorted(output['repetitions'].items()))
    lines = ['# Hashline benchmark report', '', f"Mode: {output['mode']}; {counted}; acceptance: **{str(output['acceptance']).lower()}**.",
             '', 'Times with content proof are conservative ScreenCast capture bounds. Missing, unsupported and diagnostic results never pass budgets.', '']
    if output.get('progress'):
        progress = output['progress']
        lines += [f"Progress: {progress['done']}/{progress['total']}; remaining ~{human(progress['estimatedRemainingSeconds'])}; session: {output.get('conditions', {}).get('sessionKind', 'unknown')}; stoppedBy: {output.get('stoppedBy', '—')}.", '']
    for role, title in [('self', 'Hashline'), ('peer', 'Competitors'), ('reference', 'Okular — separate reference')]:
        cells = [cell for cell in output['summary'] if output['viewers'][cell['viewer']]['role'] == role]
        if not cells:
            continue
        lines += [f'## {title}', '', '| Viewer / renderer | Group | Fixture / Hz | n | Status | Metrics | Budgets |',
                  '| --- | --- | --- | ---: | --- | --- | --- |']
        for cell in cells:
            metrics = '; '.join(f"{key}: {value['median']:.2f} / {value['p95']:.2f} (median/p95)" for key, value in cell['metrics'].items()) or 'no result'
            budgets = '; '.join(f"{b['metric']}: {b['status']}" for b in cell['budgets']) or 'no acceptance budget'
            lines.append(f"| {cell['viewer']} / {cell['renderer']} | {cell['group']} | {cell['fixture']} / {cell['refreshHz'] or '—'} | {cell['n']} | {cell['statuses']} | {metrics} | {budgets} |")
        lines.append('')
    failures = sorted({f"{r['viewer']}/{r['group']}: {r.get('reason', r['status'])}" for r in output['rows'] if r['status'] != 'ok'})
    if failures:
        lines += ['## Missing evidence', '', *['- ' + failure.replace('\n', ' ') for failure in failures], '']
    lines += ['Full provenance, raw artifact paths, warmups and budget definitions: `report.json`.', '']
    return '\n'.join(lines)


def main():
    if len(sys.argv) > 1 and sys.argv[1] in ('promote', 'list', 'prune', 'check-results'):
        from storage import cli
        return cli(sys.argv[1:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['bench', 'compare'], nargs='?', default='bench')
    parser.add_argument('--resume', help='Resume a local run by name or path, keeping its matrix and conditions')
    parser.add_argument('--time-budget', type=duration, help='Stop at a complete pass boundary, e.g. 60m')
    parser.add_argument('--watchdog-factor', type=float, default=6, help='Maximum measurement duration as a multiple of its estimate')
    parser.add_argument('--session', choices=['bare-metal', 'nested-headless'], default='bare-metal')
    parser.add_argument('--resolution', default='1920x1080', help='Fixed nested display resolution')
    parser.add_argument('--quiet-seconds', type=float, default=5, help='Preflight idle interval for physical desktop evidence')
    parser.add_argument('--only')
    parser.add_argument('--fixtures')
    parser.add_argument('--viewers')
    parser.add_argument('--renderers', default='cairo,vulkan')
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--repetitions', help='A count for every group, or per-group budgets: '
                        '"30", "content=30,memory=8", or "20,content=30"')
    parser.add_argument('--out', type=Path)
    parser.add_argument('--binary', type=Path, default=ROOT.parent / 'target/release/hashline')
    parser.add_argument('--provision', action='store_true')
    parser.add_argument('--connector', default='DP-3')
    parser.add_argument('--refresh-hz', type=float, help='Required existing refresh rate for the entire run; never switches modes')
    parser.add_argument('--hold', type=float, default=3)
    parser.add_argument('--settle', type=float, default=5)
    parser.add_argument('--sample-seconds', type=float, help='Memory observation window; PSS is stable within a second')
    parser.add_argument('--idle-seconds', type=float, help='Idle-CPU observation window; the target names 30 s')
    parser.add_argument('--idle-repetitions', type=int, help='Repetitions of the idle window (default 5)')
    parser.add_argument('--plan', action='store_true', help='Write the selected matrix without opening viewers')
    args = parser.parse_args()
    previous = None
    if args.resume:
        if args.plan or args.out or args.provision:
            parser.error('--resume cannot be combined with --plan, --out or --provision')
        # Selection and measurement options must not silently alter an old matrix.
        immutable = {'--only', '--fixtures', '--viewers', '--renderers', '--repetitions', '--quick',
                     '--binary', '--connector', '--refresh-hz', '--hold', '--settle', '--sample-seconds',
                     '--idle-seconds', '--idle-repetitions', '--session', '--resolution', '--quiet-seconds'}
        if any(arg.split('=')[0] in immutable for arg in sys.argv[1:]):
            parser.error('--resume preserves selection, binaries and conditions; omit matrix overrides')
        try:
            args.out = resolve_run(args.resume)
            previous = json.loads((args.out / 'report.json').read_text())
            if previous.get('plan'):
                parser.error('A plan is not a measured run; start a new run')
            saved = previous.get('invocation')
            if saved:
                for key, value in saved.items():
                    setattr(args, key, Path(value) if key == 'binary' else value)
            else:
                args.mode = previous['mode']
                if previous['viewers'].get('hashline', {}).get('binary'):
                    args.binary = Path(previous['viewers']['hashline']['binary'])
                args.only = ','.join(previous['repetitions'])
                args.fixtures = ','.join(dict.fromkeys(r['fixture'] for r in previous['rows']))
                args.viewers = ','.join(previous['viewers'])
                args.renderers = ','.join(dict.fromkeys(r['renderer'] for r in previous['rows'] if r['viewer'] == 'hashline')) or 'cairo'
                args.repetitions = ','.join(f'{g}={n}' for g, n in previous['repetitions'].items())
                args.quick = previous.get('quick', False)
                conditions = previous['conditions']
                for option, field in [('hold', 'holdSeconds'), ('settle', 'settleSeconds'), ('sample_seconds', 'sampleSeconds'),
                                      ('idle_seconds', 'idleSeconds'), ('connector', 'connector'), ('refresh_hz', 'requestedRefreshHz')]:
                    if field in conditions:
                        setattr(args, option, conditions[field])
        except (OSError, ValueError) as error:
            parser.error(str(error))
    configuration = catalog()
    catalog_viewers = configuration['viewers']
    catalog_viewers['hashline'] = {'name': 'Hashline', 'role': 'self', 'version': 'workspace',
                          'renderer': 'GSK', 'backend': 'Wayland', 'reload': True, 'tabs': True}
    fixture_records = {row['name']: row for row in json.loads(MANIFEST.read_text())}
    fixtures = list(fixture_records)
    try:
        groups = selected(args.only, GROUPS)
        names = selected(args.fixtures or ('small' if args.quick else None), fixtures)
        viewers = selected(args.viewers or ('hashline' if args.mode == 'bench' else None), catalog_viewers)
        renderers = selected(args.renderers, ['cairo', 'vulkan', 'gl', 'default'])
    except argparse.ArgumentTypeError as error:
        parser.error(str(error))
    if args.mode == 'bench' and viewers != ['hashline']:
        parser.error('bench measures hashline; use compare for other viewers')
    args.sample_seconds = args.sample_seconds if args.sample_seconds is not None else (2 if args.quick else 5)
    args.idle_seconds = args.idle_seconds if args.idle_seconds is not None else (2 if args.quick else 30)
    args.idle_repetitions = args.idle_repetitions if args.idle_repetitions is not None else REQUIRED['idle']
    try:
        repetitions = plan_repetitions(args.repetitions, groups, 5 if args.quick else SERIES,
                                       args.idle_repetitions)
    except argparse.ArgumentTypeError as error:
        parser.error(str(error))
    if args.refresh_hz is not None and (not math.isfinite(args.refresh_hz) or args.refresh_hz <= 0):
        parser.error('--refresh-hz must be a finite positive number')
    if any(not math.isfinite(v) or v <= 0 for v in (args.hold, args.settle, args.sample_seconds, args.idle_seconds, args.watchdog_factor, args.quiet_seconds)):
        parser.error('Durations must be positive')
    out = (args.out or RUNS / datetime.datetime.now().strftime('%Y%m%d-%H%M%S-%f')).resolve()
    if out.is_relative_to(RESULTS.resolve()):
        parser.error('Measurements are local; only promote writes benchmarks/results')
    out.mkdir(parents=True, exist_ok=True)
    if (out / 'report.json').exists() and not previous:
        parser.error('Output exists; choose a new directory')
    if args.idle_repetitions < 1:
        parser.error('--idle-repetitions must be positive')
    if args.plan and args.provision:
        parser.error('--plan cannot provision programs')
    ensure(ROOT / 'generated')
    provisioning = []
    if args.provision:
        for name, spec in configuration.get('tools', {}).items():
            print('Provisioning tool', name, flush=True)
            provisioning.append(tool(name, spec))
        for name in viewers:
            if name != 'hashline':
                print('Provisioning', name, flush=True)
                provisioning.append(provision(name, catalog_viewers[name]))
    specs = {}
    matrix_viewers = []
    for name in viewers:
        spec = dict(catalog_viewers[name])
        binary = args.binary.resolve() if name == 'hashline' else locate(spec)
        spec['binary'] = str(binary) if binary else None
        spec['binarySha256'] = hashlib.sha256(binary.read_bytes()).hexdigest() if binary and binary.is_file() else None
        if name != 'hashline' and binary:
            source_dir = LOCAL / spec['directory']
            if (source_dir / '.git').exists():
                head = subprocess.run(['git', '-C', str(source_dir), 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()
                spec['observedRevision'] = head
                spec['sourceDirty'] = subprocess.run(['git', '-C', str(source_dir), 'status', '--porcelain', '--untracked-files=no'], capture_output=True, text=True).stdout.strip()
                spec['versionVerified'] = head == spec.get('revision') and not spec['sourceDirty']
            else:
                from provision import installed
                spec['versionVerified'] = installed(spec)
        specs[name] = spec
        matrix_viewers.extend((name, renderer) for renderer in (renderers if name == 'hashline' else ['native']))
    output = {'schemaVersion': 2, 'mode': args.mode, 'repetitions': repetitions, 'quick': args.quick,
              'acceptance': False, 'completed': False, 'viewers': specs, 'provisioning': provisioning,
              'fixtureManifestSha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
              'budgetDefinitions': BUDGETS, 'rows': [], 'summary': [],
              'contentProofEngine': engine(),
              'conditions': {'connector': args.connector, 'requestedRefreshHz': args.refresh_hz,
                             'displayConfigurationChanged': False, 'holdSeconds': args.hold, 'settleSeconds': args.settle,
                             'sampleSeconds': args.sample_seconds, 'idleSeconds': args.idle_seconds,
                             'requiredRepetitions': {group: REQUIRED.get(group, SERIES) for group in groups},
                             'warmupPasses': 1,
                             'referenceMachineVerified': False, 'desktopUnattendedVerified': False},
              'git': subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT.parent, capture_output=True, text=True).stdout.strip(),
              'workingTree': subprocess.run(['git', 'status', '--porcelain'], cwd=ROOT.parent, capture_output=True, text=True).stdout,
              'plan': args.plan}
    matrix = list(schedule(matrix_viewers, groups, names, repetitions, refresh_hz=args.refresh_hz))
    output['matrix'] = matrix
    output['invocation'] = {key: str(value) if isinstance(value, Path) else value
                            for key, value in vars(args).items()
                            if key not in ('resume', 'out', 'time_budget', 'watchdog_factor', 'plan', 'provision')}
    output['createdAt'] = utcnow()
    if not args.plan and args.binary.is_file():
        from environment import environment
        output['environment'] = environment(args.binary)
    if previous:
        if previous['fixtureManifestSha256'] != output['fixtureManifestSha256']:
            parser.error('Fixture manifest changed since original run')
        for name, spec in specs.items():
            if spec['binarySha256'] != previous['viewers'][name]['binarySha256']:
                parser.error(f'Executable changed since original run: {name}')
        if previous.get('matrix') and previous['matrix'] != json.loads(json.dumps(matrix)):
            parser.error('Resume matrix differs from original run')
        output.update(previous)
        output.update(matrix=matrix, completed=False, schemaVersion=2)
        output.pop('stoppedBy', None)
        output.pop('interrupted', None)
    estimates = Estimates()
    done = {row_identity(row) for row in output['rows']}
    if len(done) != len(output['rows']) or not done <= {identity(item) for item in matrix}:
        parser.error('Existing rows are duplicated or outside the matrix')
    initial_estimate = sum(estimates.seconds(item) for item in matrix if identity(item) not in done)
    output['estimatedTotalSeconds'] = sum(estimates.seconds(item) for item in matrix)
    output['estimateSource'] = 'Local cell means after two measurements; otherwise the priors in timing.py'
    output.setdefault('segments', []).append({'startedAt': utcnow(), 'timeBudgetSeconds': args.time_budget})
    segment = output['segments'][-1]
    started = time.monotonic()
    last_save = 0
    active = None

    def save(force=True):
        nonlocal last_save
        now = time.monotonic()
        if not force and now - last_save < 1:
            return
        last_save = now
        remaining = sum(estimates.seconds(item) for item in matrix if args.plan or identity(item) not in done)
        active_elapsed = now - active['clock'] if active else 0
        remaining = max(0, remaining - min(active_elapsed, active['estimatedSeconds'] if active else 0))
        progress_done = 0 if args.plan else len(done)
        progress = {'done': progress_done, 'total': len(matrix), 'remaining': len(matrix) - progress_done,
                    'elapsedSeconds': now - started, 'estimatedRemainingSeconds': remaining,
                    'measuredSeconds': sum(r.get('durationSeconds', 0) for r in output['rows']),
                    'estimatedTotalSeconds': now - started + remaining,
                    'updatedAt': utcnow(), 'active': {k: v for k, v in active.items() if k != 'clock'} if active else None}
        output['progress'] = progress
        output['summary'] = summarize(output['rows'], repetitions)
        output['acceptanceEligible'] = (not args.quick and not args.plan and output['completed']
                                       and bool(output['summary']) and all(cell['complete'] for cell in output['summary'])
                                       and all(r.get('proofResolutionValid') is True for r in output['rows']
                                               if not r['warmup'] and r['group'] in SCREEN_GROUPS - {'scroll'}))
        output['acceptance'] = accepted(output['summary'], output['acceptanceEligible'])
        temporary = out / 'report.tmp'
        temporary.write_text(json.dumps(output, indent=2) + '\n')
        temporary.replace(out / 'report.json')
        (out / 'report.md').write_text(report(output))
        line = (f"{progress['done']}/{progress['total']} | elapsed {human(progress['elapsedSeconds'])}"
                f" | remaining ~{human(remaining)} | total ~{human(progress['estimatedTotalSeconds'])}")
        if active:
            line += ' | ' + active['identity']
        print(('\r\033[K' if sys.stdout.isatty() else '') + line,
              end='' if sys.stdout.isatty() else '\n', flush=True)

    def terminated(signum, frame):
        raise KeyboardInterrupt

    run_lock = locked(out)
    try:
        run_lock.__enter__()
    except ValueError as error:
        parser.error(str(error))
    previous_signal = signal.signal(signal.SIGTERM, terminated)
    from session import Session
    try:
        with Session(args.session, args.resolution, out, enabled=not args.plan) as session:
            if session.connector:
                args.connector = session.connector
            output['conditions']['sessionKind'] = args.session
            output['conditions']['resolution'] = args.resolution if args.session == 'nested-headless' else None
            if not args.plan:
                from session import quiet_check
                quiet = quiet_check(args.quiet_seconds) if args.session == 'bare-metal' and set(groups) & SCREEN_GROUPS else {
                    'verified': args.session == 'nested-headless', 'reason': 'Isolated headless display' if args.session == 'nested-headless' else 'No monitor evidence requested'}
                output['conditions']['desktopQuietCheck'] = quiet
                output['conditions']['desktopUnattendedVerified'] = quiet['verified']
                if 'environment' in output:
                    output['environment']['desktopUnattendedVerified'] = quiet['verified']
                    output['environment']['desktopCheckSessionKind'] = args.session
            save()
            if args.plan:
                print(f'Estimated selected matrix: {human(initial_estimate)}')
            current_pass = None
            for item in matrix:
                iteration, group, fixture, hz, (name, renderer) = item
                if identity(item) in done:
                    continue
                # A budget only stops at a pass boundary. If a resumed pass was
                # partial, finish it before applying a budget decision.
                partial = any(r['iteration'] == iteration for r in output['rows'])
                if current_pass != iteration and not partial and args.time_budget and not args.plan:
                    pass_estimate = sum(estimates.seconds(m) for m in matrix if m[0] == iteration and identity(m) not in done)
                    if time.monotonic() - started + pass_estimate > args.time_budget:
                        output['stoppedBy'] = 'time-budget'
                        break
                current_pass = iteration
                label = f'{iteration + 1:03}-{name}-{renderer}-{group}-{fixture}' + (f'-{hz}' if hz else '')
                artifact = out / 'raw' / label
                row = {'iteration': iteration, 'warmup': iteration < 0, 'viewer': name, 'renderer': renderer,
                       'rendererDeclared': renderer if name == 'hashline' else specs[name]['renderer'],
                       'rendererObserved': 'unknown', 'backendRequested': 'Wayland',
                       'group': group, 'fixture': fixture, 'fixtureSha256': fixture_records[fixture]['sha256'],
                       'refreshHz': hz, 'artifact': str(artifact), 'metrics': {},
                       'startedAt': utcnow(), 'sessionKind': args.session,
                       'desktopUnattendedVerified': output['conditions']['desktopUnattendedVerified']}
                command = [specs[name]['binary'], *specs[name].get('args', [])]
                row_start = time.monotonic()
                active = {'identity': label, 'startedAt': row['startedAt'], 'estimatedSeconds': estimates.seconds(item), 'clock': row_start}
                if not args.plan:
                    save()
                try:
                    if args.plan:
                        result = {'status': 'planned', 'reason': 'Plan only; no measurement'}
                    elif args.session == 'nested-headless' and group in ('scroll', 'startup'):
                        result = {'status': 'unsupported', 'reason': 'Physical presentation requires a bare-metal monitor'}
                    elif group in SCREEN_GROUPS and not row['desktopUnattendedVerified']:
                        result = {'status': 'missing', 'reason': 'Desktop idle preflight failed; use --session nested-headless or leave the desktop idle'}
                    elif name != 'hashline' and group in INTERNAL:
                        result = {'status': 'unsupported', 'reason': 'Requires Hashline instrumentation'}
                    elif not specs[name]['binarySha256']:
                        result = {'status': 'missing', 'reason': 'Pinned executable unavailable; use --provision'}
                    elif name != 'hashline' and not specs[name].get('versionVerified'):
                        result = {'status': 'missing', 'reason': 'Source/package version differs from competitors.toml'}
                    else:
                        row['attempts'] = []
                        for attempt in range(3):
                            attempt_path = artifact / f'attempt-{attempt + 1}'
                            attempt_start = time.monotonic()
                            attempt_utc = utcnow()
                            options = {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()}
                            request = {'group': group, 'command': command, 'fixture': str(ROOT / 'generated' / (fixture + '.md')),
                                       'renderer': renderer if name == 'hashline' else 'default', 'spec': specs[name],
                                       'options': options, 'artifact': str(attempt_path), 'hz': hz}
                            # This bound covers all retries, not a fresh allowance per retry.
                            limit = max(30, estimates.seconds(item) * args.watchdog_factor) - (time.monotonic() - row_start)
                            result = watchdog(request, attempt_path, max(.01, limit), lambda: save(False))
                            row['attempts'].append({'attempt': attempt + 1, 'startedAt': attempt_utc,
                                                    'durationSeconds': time.monotonic() - attempt_start,
                                                    'artifact': str(attempt_path), 'status': result['status'],
                                                    'reason': result.get('reason'), 'proofFailure': result.get('proofFailure')})
                            failure = result.get('proofFailure') or {}
                            if not (result['status'] == 'missing' and failure.get('kind') == 'text-not-recognized'
                                    and failure.get('capturedFrames', 0) > 0):
                                break
                    row.update(result)
                    expected_renderer = {'cairo': 'GskCairoRenderer', 'vulkan': 'GskVulkanRenderer'}.get(renderer)
                    observed = row.get('rendererObserved', 'unknown')
                    if name == 'hashline' and expected_renderer and observed != 'unknown' and observed != expected_renderer:
                        row.update(status='missing', reason=f'Renderer fallback: requested {renderer}, observed {observed}')
                except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, ImportError) as error:
                    row.update(status='missing', reason=str(error))
                row['durationSeconds'] = time.monotonic() - row_start
                if row['status'] not in ('ok', 'planned') and not row.get('reason'):
                    row['reason'] = 'Measurement adapter supplied no usable evidence; see local artifacts'
                row['command'] = [*command, str(ROOT / 'generated' / (fixture + '.md'))]
                row['acceptance'] = False
                output['rows'].append(row)
                done.add(identity(item))
                estimates.record(row)
                active = None
                if not args.plan:
                    save()
            output['completed'] = len(done) == len(matrix) and not args.plan
    except KeyboardInterrupt:
        output['stoppedBy'] = 'interrupted'
        if active:
            output.setdefault('interruptedMeasurements', []).append({k: v for k, v in active.items() if k != 'clock'} |
                                                                   {'durationSeconds': time.monotonic() - active['clock']})
    except Exception as error:
        output['stoppedBy'] = 'error'
        output['error'] = f'{type(error).__name__}: {error}'
    finally:
        signal.signal(signal.SIGTERM, previous_signal)
        segment['durationSeconds'] = time.monotonic() - started
        segment['stoppedBy'] = output.get('stoppedBy', 'completed' if output['completed'] else 'plan')
        active = None
        save()
        if sys.stdout.isatty():
            print()
        run_lock.__exit__(None, None, None)
    print(out / 'report.md')
    return 0 if args.plan or output.get('stoppedBy') == 'time-budget' or (output['completed'] and all(
        r['status'] in ('ok', 'unsupported', 'diagnostic') for r in output['rows'])) else 1


if __name__ == '__main__':
    raise SystemExit(main())
