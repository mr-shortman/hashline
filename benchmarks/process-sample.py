"""Sample the complete app process group: python3 benchmarks/process-sample.py PID [seconds]."""
import json
import os
from pathlib import Path
import sys
import time

root_pid = int(sys.argv[1])
duration = int(sys.argv[2]) if len(sys.argv) > 2 else 30
ticks = os.sysconf("SC_CLK_TCK")
samples = []

def process_tree():
    processes = {}
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / 'stat').read_text().rsplit(')', 1)[1].split()
            processes[int(entry.name)] = (int(fields[1]), int(fields[11]) + int(fields[12]))
        except (OSError, ValueError):
            continue
    selected = {root_pid}
    while True:
        extra = {pid for pid, (parent, _) in processes.items() if parent in selected}
        expanded = selected | extra
        if expanded == selected:
            break
        selected = expanded
    return {pid: processes[pid][1] for pid in selected if pid in processes}

start = time.monotonic()
while time.monotonic() - start < duration:
    processes = process_tree()
    pss = 0
    readable = True
    for pid in processes:
        try:
            for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines():
                if line.startswith('Pss:'):
                    pss += int(line.split()[1])
        except OSError:
            readable = False
    samples.append({'seconds': time.monotonic() - start, 'pids': list(processes), 'pssKiB': pss if readable else None, 'cpuTicks': sum(processes.values()), 'cpuTicksByPid': processes})
    time.sleep(1)
stable_group = len({tuple(sorted(sample['pids'])) for sample in samples}) == 1
cpu = (samples[-1]['cpuTicks'] - samples[0]['cpuTicks']) / ticks / (samples[-1]['seconds'] - samples[0]['seconds']) * 100 if len(samples) > 1 and stable_group else None
print(json.dumps({'method': 'Linux /proc/smaps_rollup PSS, recursive children. CPU is unavailable when process-group membership changes, avoiding negative or understated deltas.', 'stableProcessGroup': stable_group, 'cpuPercentOfOneCore': cpu, 'samples': samples}, indent=2))
