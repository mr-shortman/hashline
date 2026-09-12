"""Settled PSS and CPU of the release reader after 50 real file switches.

docs/metrics.md: after fifty file changes no continuous growth may remain, and
once things have settled the consumption must be at most 20 % above the warmed
baseline.

A switch is a real one. The reader is single-instance, so invoking the binary
again hands the file to the running window exactly as a file manager would —
there is no test hook, and nothing in the application is instrumented.
"""
import argparse
import json
from pathlib import Path
import os
import statistics
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from environment import environment  # noqa: E402
from processes import sample  # noqa: E402


def switch(binary, path):
    """Hand one file to the running instance."""
    return subprocess.run([binary, str(path)], capture_output=True, timeout=30).returncode


def settled(pid, seconds, label):
    """Samples the recursive process group once a second and reduces it."""
    print(f'{label}: sampling {seconds}s', flush=True)
    start = time.monotonic()
    samples = [sample(pid, start)]
    while time.monotonic() - start < seconds:
        time.sleep(1.0)
        samples.append(sample(pid, start))
    values = [row['pssKiB'] for row in samples if row['pssKiB'] is not None]
    ticks = [row['cpuTicks'] for row in samples]
    span = samples[-1]['seconds'] - samples[0]['seconds']
    # An unreadable PSS is not zero consumption; it is reported as unknown.
    return {
        'samples': samples,
        'pssKibMedian': statistics.median(values) if len(values) == len(samples) else None,
        'pssKibSeries': values if len(values) == len(samples) else None,
        'cpuTicksDelta': ticks[-1] - ticks[0],
        'cpuCoreShare': ((ticks[-1] - ticks[0]) / os.sysconf('SC_CLK_TCK') / span) if span > 0 else None,
        'spanSeconds': span,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('--first', type=Path, default=Path('benchmarks/generated/small.md'))
    parser.add_argument('--second', type=Path, default=Path('benchmarks/generated/medium.md'))
    parser.add_argument('--warmup', type=int, default=10)
    parser.add_argument('--switches', type=int, default=50)
    parser.add_argument('--settle', type=float, default=0.35)
    parser.add_argument('--sample-seconds', type=int, default=15)
    parser.add_argument('--output', type=Path,
                        default=Path('benchmarks/.local/runs/native-stability/report.json'))
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

    binary_name = Path(args.binary).name
    if subprocess.run(['pgrep', '-x', binary_name], capture_output=True).returncode == 0:
        parser.error(f'{binary_name} is already running; start from a clean state')

    output = {'environment': environment(args.binary), 'warmupSwitches': args.warmup,
              'switches': args.switches, 'completed': False,
              'method': 'Real single-instance handoff per switch; recursive process-group '
                        'PSS/CPU over /proc. No application instrumentation.'}
    application = subprocess.Popen([args.binary, str(args.first)],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(4.0)
        if application.poll() is not None:
            output['error'] = 'application exited at startup'
            return 1

        failures = 0
        for index in range(args.warmup):
            failures += switch(args.binary, args.second if index % 2 == 0 else args.first)
            time.sleep(args.settle)
        output['warmupFailures'] = failures
        output['baseline'] = settled(application.pid, args.sample_seconds, 'warmed baseline')

        failures = 0
        for index in range(args.switches):
            failures += switch(args.binary, args.second if index % 2 == 0 else args.first)
            time.sleep(args.settle)
        output['switchFailures'] = failures
        output['after'] = settled(application.pid, args.sample_seconds, 'after switches')

        base = output['baseline'].get('pssKibMedian')
        after = output['after'].get('pssKibMedian')
        if base and after:
            output['growthPercent'] = 100 * (after - base) / base
            output['withinTwentyPercent'] = output['growthPercent'] <= 20
        # A rising trend inside the final sample would mean it never settled.
        series = output['after'].get('pssKibSeries') or []
        if len(series) >= 4:
            half = len(series) // 2
            first_half = statistics.median(series[:half])
            second_half = statistics.median(series[half:])
            output['trendPercentWithinFinalSample'] = 100 * (second_half - first_half) / first_half
        output['alive'] = application.poll() is None
        output['completed'] = output['alive'] and failures == 0
    finally:
        application.terminate()
        try:
            application.wait(timeout=5)
        except subprocess.TimeoutExpired:
            application.kill()
        args.output.write_text(json.dumps(output, indent=2) + '\n')
        print(json.dumps({k: v for k, v in output.items()
                          if k not in ('environment', 'baseline', 'after')}, indent=2))
    return 0 if output['completed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
