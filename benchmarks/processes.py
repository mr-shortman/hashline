"""Recursive Linux process-group samples shared by idle and image benchmarks."""
import os
from pathlib import Path
import threading
import time


def process_tree(root_pid):
    processes = {}
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / 'stat').read_text().rsplit(')', 1)[1].split()
            processes[int(entry.name)] = (int(fields[1]), int(fields[11]) + int(fields[12]), int(fields[19]))
        except (OSError, ValueError):
            continue
    selected = {root_pid}
    while True:
        expanded = selected | {pid for pid, (parent, _, _) in processes.items() if parent in selected}
        if expanded == selected:
            break
        selected = expanded
    return {pid: processes[pid][1:] for pid in selected if pid in processes}


def sample(root_pid, start):
    processes = process_tree(root_pid)
    pss = 0
    readable = root_pid in processes and bool(processes)
    for pid in processes:
        try:
            values = [int(line.split()[1]) for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines() if line.startswith('Pss:')]
            if len(values) != 1:
                readable = False
            pss += sum(values)
        except OSError:
            readable = False
    return {'seconds': time.monotonic() - start, 'pids': list(processes),
            'processIdentities': sorted([pid, values[1]] for pid, values in processes.items()),
            'pssKiB': pss if readable else None, 'cpuTicks': sum(v[0] for v in processes.values()),
            'cpuTicksByPid': {pid: v[0] for pid, v in processes.items()}}


def app_pid(binary):
    path = str(Path(binary).resolve())
    pids = [int(p.name) for p in Path('/proc').iterdir()
            if p.name.isdigit() and os.path.realpath(p / 'exe') == path]
    if len(pids) != 1:
        raise RuntimeError(f'Expected one app process, found {pids}')
    return pids[0]


class ImageMemory:
    """Sampled PSS at 250 ms; this is not an allocation-level decode peak."""
    def __init__(self, binary):
        self.pid = app_pid(binary)
        self.start = time.monotonic()
        self.samples = []
        self.stop_event = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        while True:
            self.samples.append(sample(self.pid, self.start))
            if self.stop_event.wait(.25):
                return

    def finish(self):
        self.stop_event.set()
        self.thread.join()
        self.samples.append(sample(self.pid, self.start))
        values = [s['pssKiB'] for s in self.samples if s['pssKiB'] is not None]
        return {'method': 'App-process-group PSS sampled every 250 ms plus final sample; allocation/decode peaks between samples may be missed.',
                'samples': self.samples,
                'sampledPeakPssMiB': max(values) / 1024 if len(values) == len(self.samples) else None}
