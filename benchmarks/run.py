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

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
from content import engine
from fixtures import MANIFEST, ensure
from provision import LOCAL, catalog, locate, provision, tool
from suite import measure

GROUPS = ('stages', 'startup', 'content', 'memory', 'idle', 'scroll', 'interaction', 'tabs', 'reload', 'stability')
INTERNAL = {'stages', 'interaction', 'stability'}
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
    'mainThreadMaxMs': ('interaction', '<=', 16, 'max'),
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
            if isinstance(limits, list) and fixture not in SIZE:
                continue
            limit = limits[SIZE[fixture]] if isinstance(limits, list) and fixture in SIZE else limits
            if isinstance(limit, list):
                limit = None
            if name == 'inactiveTabMiB':
                limit = 3 * (ROOT / 'generated' / (fixture + '.md')).stat().st_size / 1048576
            value = metrics.get(name, {}).get(statistic)
            known = value is not None and limit is not None and complete and metrics[name]['n'] == expected
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
             '', 'Times with content proof are conservative ScreenCast receipt bounds. Missing, unsupported and diagnostic results never pass budgets.', '']
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['bench', 'compare'])
    parser.add_argument('--only')
    parser.add_argument('--fixtures')
    parser.add_argument('--viewers')
    parser.add_argument('--renderers', default='cairo,vulkan')
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--repetitions', type=int)
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
    configuration = catalog()
    catalog_viewers = configuration['viewers']
    catalog_viewers['hashline'] = {'name': 'Hashline', 'role': 'self', 'version': 'workspace',
                          'renderer': 'GSK', 'backend': 'Wayland', 'reload': True, 'tabs': False}
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
    args.repetitions = args.repetitions if args.repetitions is not None else (5 if args.quick else SERIES)
    args.sample_seconds = args.sample_seconds if args.sample_seconds is not None else (2 if args.quick else 5)
    args.idle_seconds = args.idle_seconds if args.idle_seconds is not None else (2 if args.quick else 30)
    args.idle_repetitions = args.idle_repetitions if args.idle_repetitions is not None else REQUIRED['idle']
    repetitions = {group: min(args.repetitions, args.idle_repetitions) if group == 'idle' else args.repetitions
                   for group in groups}
    if args.refresh_hz is not None and (not math.isfinite(args.refresh_hz) or args.refresh_hz <= 0):
        parser.error('--refresh-hz must be a finite positive number')
    if min(repetitions.values()) < 1 or min(args.hold, args.settle, args.sample_seconds, args.idle_seconds) <= 0:
        parser.error('Repetitions and durations must be positive')
    out = (args.out or ROOT / 'results' / ('local-' + datetime.datetime.now().strftime('%Y%m%d-%H%M%S'))).resolve()
    out.mkdir(parents=True, exist_ok=True)
    if (out / 'report.json').exists():
        parser.error('Output exists; choose a new directory')
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
    output = {'schemaVersion': 1, 'mode': args.mode, 'repetitions': repetitions, 'quick': args.quick,
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
    if not args.plan and args.binary.is_file():
        from environment import environment
        output['environment'] = environment(args.binary)
    def save():
        output['summary'] = summarize(output['rows'], repetitions)
        output['acceptanceEligible'] = (not args.quick and not args.plan and output['completed']
                                        and all(cell['complete'] for cell in output['summary']))
        output['acceptance'] = accepted(output['summary'], output['acceptanceEligible'])
        temporary = out / 'report.tmp'
        temporary.write_text(json.dumps(output, indent=2) + '\n'); temporary.replace(out / 'report.json')
        (out / 'report.md').write_text(report(output))
    save()
    blocked = {}
    display_failure = None
    if args.refresh_hz is not None and not args.plan:
        try:
            from display import verify_rate
            output['conditions']['display'] = verify_rate(args.refresh_hz, args.connector)
        except Exception as error:
            display_failure = str(error)
            output['conditions']['displayError'] = display_failure
    try:
        for iteration, group, fixture, hz, (name, renderer) in schedule(matrix_viewers, groups, names, repetitions, refresh_hz=args.refresh_hz):
            identity = f'{iteration + 1:03}-{name}-{renderer}-{group}-{fixture}' + (f'-{hz}' if hz else '')
            artifact = out / 'raw' / identity
            row = {'iteration': iteration, 'warmup': iteration < 0, 'viewer': name, 'renderer': renderer,
                   'rendererDeclared': renderer if name == 'hashline' else specs[name]['renderer'],
                   'rendererObserved': 'unknown', 'backendRequested': 'Wayland',
                   'group': group, 'fixture': fixture, 'fixtureSha256': fixture_records[fixture]['sha256'],
                   'refreshHz': hz, 'artifact': str(artifact), 'metrics': {}}
            command = [specs[name]['binary'], *specs[name].get('args', [])]
            key = name, renderer, group, fixture, hz
            try:
                if args.plan:
                    result = {'status': 'planned', 'reason': 'Plan only; no measurement'}
                elif display_failure:
                    result = {'status': 'missing', 'reason': display_failure}
                elif key in blocked:
                    result = {'status': 'missing', 'reason': 'Preflight failed: ' + blocked[key]}
                elif name != 'hashline' and group in INTERNAL:
                    result = {'status': 'unsupported', 'reason': 'Requires Hashline instrumentation'}
                elif not specs[name]['binarySha256']:
                    result = {'status': 'missing', 'reason': 'Pinned executable unavailable; use --provision'}
                elif name != 'hashline' and not specs[name].get('versionVerified'):
                    result = {'status': 'missing', 'reason': 'Source/package version differs from competitors.toml'}
                else:
                    if args.refresh_hz is not None:
                        from display import verify_rate
                        row['display'] = verify_rate(args.refresh_hz, args.connector)
                    print(identity, flush=True)
                    result = measure(group, command, ROOT / 'generated' / (fixture + '.md'),
                                     renderer if name == 'hashline' else 'default', specs[name], args, artifact, hz)
                row.update(result)
                expected_renderer = {'cairo': 'GskCairoRenderer', 'vulkan': 'GskVulkanRenderer'}.get(renderer)
                observed_renderer = row.get('rendererObserved', 'unknown')
                if name == 'hashline' and expected_renderer and observed_renderer != 'unknown' and observed_renderer != expected_renderer:
                    row.update(status='missing', reason=f'Renderer fallback: requested {renderer}, observed {observed_renderer}')
            except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, ImportError) as error:
                row.update(status='missing', reason=str(error))
                # Only unchanging setup failures are cached. Timeouts/crashes get a fresh attempt.
                if isinstance(error, (FileNotFoundError, ImportError)):
                    blocked[key] = str(error)
            row['command'] = [*command, str(ROOT / 'generated' / (fixture + '.md'))]
            row['acceptance'] = False  # A single sample is never an acceptance.
            output['rows'].append(row)
            if not args.plan:
                save()  # A crash mid-run must still leave every finished measurement.
        output['completed'] = True
    except KeyboardInterrupt:
        output['interrupted'] = True
    finally:
        save()
    print(out / 'report.md')
    return 0 if output['completed'] and all(r['status'] in ('ok', 'unsupported', 'planned', 'diagnostic') for r in output['rows']) else 1


if __name__ == '__main__':
    raise SystemExit(main())
