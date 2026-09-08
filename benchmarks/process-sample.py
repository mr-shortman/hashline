"""Sample the complete app process group: python3 benchmarks/process-sample.py PID [seconds]."""
import json
import os
import sys
import time
from processes import sample

root_pid = int(sys.argv[1])
duration = int(sys.argv[2]) if len(sys.argv) > 2 else 30
if duration <= 0:
    raise SystemExit('Duration must be positive')
ticks = os.sysconf('SC_CLK_TCK')
start = time.monotonic()
samples = []
while True:
    samples.append(sample(root_pid, start))
    elapsed = samples[-1]['seconds'] - samples[0]['seconds']
    if elapsed >= duration:
        break
    time.sleep(min(1, duration - elapsed))
stable_group = all(root_pid in s['pids'] for s in samples) and all(
    s['processIdentities'] == samples[0]['processIdentities'] for s in samples)
cpu = (samples[-1]['cpuTicks'] - samples[0]['cpuTicks']) / ticks / elapsed * 100 if stable_group else None
print(json.dumps({'method': 'Linux /proc/smaps_rollup PSS, recursive children. CPU is unavailable when process membership or PID start times change. At least the requested duration between first and last sample.',
                  'stableProcessGroup': stable_group, 'cpuPercentOfOneCore': cpu, 'samples': samples}, indent=2))
