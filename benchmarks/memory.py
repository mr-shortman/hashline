"""Settled PSS and idle CPU of the reader, one isolated process per fixture.

Two things make a memory number on a desktop hard to trust, and both were in
the way of checking the GSK renderer's floor:

* Hashline is single instance. A running copy — the one installed under
  ``~/.local/bin`` — owns the bus name, so starting the binary again hands the
  file to *that* window and measures nothing.
* ``dbus-run-session`` solves the first problem and creates a worse one. The
  private bus it starts is the parent of everything it activates, so the
  portal stack, gvfs and dconf land inside the measured process group and add
  some 60 MiB that has nothing to do with the reader.

So this starts the private bus itself, as a *sibling* of the application
rather than its parent. Whatever the bus activates is a child of the bus; the
application's own process group holds the application and nothing else. The
result records the bus's process group as well, so the reading can be checked
against what was left out rather than taken on trust.

Nothing in the application is instrumented, and nothing about it is special
cased: it is the release binary, started with a file, sampled through /proc.

Usage::

    python3 benchmarks/memory.py target/release/hashline \\
        --fixture benchmarks/generated/small.md \\
        --fixture benchmarks/generated/large.md \\
        --renderer cairo --output benchmarks/.local/runs/native-memory-cairo.json

A fixture named ``-`` starts the reader with no file at all, which is what the
renderer's own floor costs.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import re
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from environment import environment  # noqa: E402
from processes import process_tree, sample  # noqa: E402

# Portals, gvfs and dconf are session services, not part of the reader. They are
# kept off the private bus entirely rather than merely excluded from the sample,
# so that they cannot be started by the run and then be missed by it.
ISOLATION = {
    'GTK_USE_PORTAL': '0',
    'GIO_USE_VFS': 'local',
    'GSETTINGS_BACKEND': 'memory',
    'NO_AT_BRIDGE': '1',
}


class PrivateBus:
    """A session bus of this run's own, started beside the application.

    The address is read from a pipe rather than guessed, so the bus is known to
    be up before the application is told about it.
    """

    def __init__(self):
        read, write = os.pipe()
        self.process = subprocess.Popen(
            ['dbus-daemon', '--session', '--nofork', f'--print-address={write}'],
            pass_fds=(write,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(write)
        with os.fdopen(read) as pipe:
            self.address = pipe.readline().strip()
        if not self.address:
            self.process.wait(timeout=5)
            raise RuntimeError('the private bus printed no address')

    def activated(self):
        """The processes the bus started: exactly what the sample leaves out."""
        return sorted(process_tree(self.process.pid))

    def stop(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()


def settled(pid, seconds, interval=1.0):
    """Samples one process group for `seconds` and reduces it."""
    start = time.monotonic()
    samples = [sample(pid, start)]
    while time.monotonic() - start < seconds:
        time.sleep(interval)
        samples.append(sample(pid, start))
    values = [row['pssKiB'] for row in samples if row['pssKiB'] is not None]
    ticks = [row['cpuTicks'] for row in samples]
    span = samples[-1]['seconds'] - samples[0]['seconds']
    return {
        'samples': samples,
        # An unreadable PSS is not zero consumption; it is reported as unknown.
        'pssKibMedian': statistics.median(values) if len(values) == len(samples) else None,
        'pssKibMax': max(values) if len(values) == len(samples) else None,
        'cpuCoreShare': ((ticks[-1] - ticks[0]) / os.sysconf('SC_CLK_TCK') / span)
                        if span > 0 else None,
        'spanSeconds': span,
    }


def measure(binary, fixture, bus, renderer, settle, seconds, home):
    """One process, one fixture, from launch to sample to exit."""
    environ = dict(os.environ)
    environ.update(ISOLATION)
    environ['DBUS_SESSION_BUS_ADDRESS'] = bus.address
    environ['HASHLINE_BENCH_METADATA'] = '1'
    # A run must not read or write the desktop's own preferences, and must not
    # inherit a reading position from an earlier one.
    for name, directory in (('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                            ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state')):
        path = Path(home) / directory
        path.mkdir(parents=True, exist_ok=True)
        environ[name] = str(path)
    if renderer:
        environ['GSK_RENDERER'] = renderer
    else:
        environ.pop('GSK_RENDERER', None)

    command = [str(binary)] + ([str(fixture)] if fixture else [])
    log = tempfile.TemporaryFile(mode='w+')
    try:
        application = subprocess.Popen(command, env=environ, start_new_session=True,
                                       stdout=subprocess.DEVNULL, stderr=log)
    except BaseException:
        log.close()
        raise
    row = {
        'fixture': str(fixture) if fixture else None,
        'bytes': Path(fixture).stat().st_size if fixture else None,
        'renderer': renderer or 'default',
        'command': command,
    }
    try:
        time.sleep(settle)
        if application.poll() is not None:
            row['error'] = f'the reader exited with {application.returncode}'
            return row
        row.update(settled(application.pid, seconds))
        row['pids'] = sorted(process_tree(application.pid))
        row['busActivatedPids'] = bus.activated()
        # The whole point of the isolation: nothing the bus started is inside
        # the group that was measured.
        row['isolated'] = not set(row['pids']) & set(row['busActivatedPids'])
        row['alive'] = application.poll() is None
    finally:
        try:
            os.killpg(application.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            application.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(application.pid, signal.SIGKILL)
            application.wait()
        log.seek(0)
        metadata = re.search(r'HASHLINE_BENCH renderer=(\S+) backend=(\S+)', log.read())
        row['rendererObserved'] = metadata[1] if metadata else 'unknown'
        row['backendObserved'] = metadata[2] if metadata else 'unknown'
        log.close()
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('binary')
    parser.add_argument('--fixture', action='append', default=[],
                        help='a Markdown file, or "-" for no file at all; repeatable')
    parser.add_argument('--renderer', action='append', default=[],
                        help='a value for GSK_RENDERER, or "default" to leave it unset; '
                             'repeatable, and every fixture is measured under each')
    parser.add_argument('--repeat', type=int, default=1,
                        help='how often to walk the whole matrix; each pass is its own '
                             'process, and the passes interleave so that a warming GPU '
                             'driver cannot be mistaken for a cheaper renderer')
    parser.add_argument('--settle', type=float, default=5.0,
                        help='seconds between the launch and the first sample')
    parser.add_argument('--sample-seconds', type=float, default=20.0)
    parser.add_argument('--output', type=Path,
                        default=Path('benchmarks/.local/runs/native-memory/report.json'))
    args = parser.parse_args()
    from storage import require_local_output
    if args.output is not None:
        try:
            require_local_output(args.output)
        except ValueError as error:
            parser.error(str(error))
    if args.output.exists():
        parser.error('Output exists; use a new path')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fixtures = args.fixture or ['-']
    renderers = args.renderer or ['default']

    binary_name = Path(args.binary).name
    running = subprocess.run(['pgrep', '-a', '-x', binary_name], capture_output=True, text=True)
    output = {
        'environment': environment(args.binary),
        'method': 'One release process per fixture on a private session bus started beside it, '
                  'not around it; recursive process-group PSS/CPU over /proc after settling. '
                  'Portals, gvfs and dconf are kept off that bus and are therefore neither '
                  'started by the run nor counted in it. No application instrumentation.',
        'isolation': ISOLATION,
        'otherInstancesAtStart': running.stdout.strip().splitlines(),
        'settleSeconds': args.settle,
        'sampleSeconds': args.sample_seconds,
        'rows': [],
        'completed': False,
    }
    # A copy running on the desktop's own bus is fine — it cannot answer for
    # this one — but it is recorded, because it competes for the machine.
    bus = PrivateBus()
    home = tempfile.mkdtemp(prefix='hashline-memory-')
    output['busAddress'] = bus.address
    try:
        for pass_index in range(args.repeat):
            for renderer in renderers:
                for fixture in fixtures:
                    path = None if fixture == '-' else Path(fixture)
                    row = measure(args.binary, path, bus,
                                  None if renderer == 'default' else renderer,
                                  args.settle, args.sample_seconds, home)
                    row['pass'] = pass_index
                    pss = row.get('pssKibMedian')
                    shown = f'{pss / 1024:8.1f} MiB' if pss else '  unreadable'
                    print(f'{pass_index}  {renderer:>8}  {fixture:<40} {shown}', flush=True)
                    output['rows'].append(row)
        output['completed'] = all(row.get('alive') and row.get('isolated')
                                  for row in output['rows'])
    finally:
        bus.stop()
        shutil.rmtree(home, ignore_errors=True)
        args.output.write_text(json.dumps(output, indent=2) + '\n')
    summary = [{k: v for k, v in row.items() if k not in ('samples', 'pids', 'busActivatedPids')}
               for row in output['rows']]
    print(json.dumps({'completed': output['completed'], 'rows': summary}, indent=2))
    return 0 if output['completed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
