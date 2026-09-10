"""Wall-clock estimates and a process-tree watchdog for complete measurements."""
import datetime
import fcntl
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent
# Decision 014 §5.2, measured small-fixture costs in seconds. Other fixtures
# and programs use these priors until two observations of their cell exist.
START = dict(stages=1, startup=4, content=6, memory=13, scroll=21, idle=38)
# Groups absent from §5.2: explicit compositions, not claimed measurements.
START.update(interaction=4 * START['content'] + 5, tabs=START['content'] + 3 * START['memory'],
             reload=START['content'] + 8, stability=180, mainthread=8)


def utcnow():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def duration(value):
    match = re.fullmatch(r'(\d+(?:\.\d+)?)(s|m|h)?', value)
    if not match:
        raise ValueError('Use a positive duration such as 60m, 2h or 30s')
    seconds = float(match[1]) * {'s': 1, 'm': 60, 'h': 3600, None: 1}[match[2]]
    if not math.isfinite(seconds) or seconds <= 0:
        raise ValueError('Duration must be finite and positive')
    return seconds


def human(seconds):
    seconds = max(0, round(seconds))
    return f'{seconds // 3600:d}h {seconds % 3600 // 60:02d}m {seconds % 60:02d}s'


def identity(item):
    iteration, group, fixture, hz, (viewer, renderer) = item
    return (iteration, group, fixture, hz, viewer, renderer)


def row_identity(row):
    return tuple(row[k] for k in ('iteration', 'group', 'fixture', 'refreshHz', 'viewer', 'renderer'))


class Estimates:
    def __init__(self, path=None):
        self.path = path or ROOT / '.local/durations.json'
        self.values = self.read()

    def read(self):
        try:
            return json.loads(self.path.read_text())
        except (FileNotFoundError, ValueError):
            return {}

    @staticmethod
    def key(group, viewer, fixture):
        return '/'.join((group, viewer, fixture))

    def seconds(self, item):
        _, group, fixture, _, (viewer, _) = item
        samples = self.values.get(self.key(group, viewer, fixture), [])
        return sum(samples) / len(samples) if len(samples) >= 2 else START[group]

    def record(self, row):
        if row['status'] in ('planned', 'unsupported', 'timeout') or not row.get('attempts'):
            return
        if row['status'] == 'missing' and not (row.get('raw') or (row.get('proofFailure') or {}).get('kind') in
                                               ('text-not-recognized', 'program-exited')):
            return  # Setup failures are not observations of the cell's normal cost.
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.with_suffix('.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.values = self.read()
            key = self.key(row['group'], row['viewer'], row['fixture'])
            values = self.values.setdefault(key, [])
            values.append(row['durationSeconds'])
            self.values[key] = values[-100:]
            temporary = self.path.with_suffix('.tmp')
            temporary.write_text(json.dumps(self.values, indent=2) + '\n')
            temporary.replace(self.path)


def watchdog(request, artifact, seconds, heartbeat=None):
    """Use a fresh interpreter: no inherited GLib threads or cached bus handles.

    Track descendants by PID and birth time, including their private sessions.
    The worker is a subreaper so double-forked children remain attributable.
    """
    from processes import process_tree
    artifact.mkdir(parents=True, exist_ok=True)
    request_path, response = artifact / 'request.json', artifact / 'measurement.json'
    request_path.write_text(json.dumps(request))
    started = time.monotonic()
    tracked = {}
    with (artifact / 'worker.log').open('w') as log:
        worker = subprocess.Popen([sys.executable, str(ROOT / 'worker.py'), str(request_path), str(response)],
                                  stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while True:
                tracked.update({pid: value[1] for pid, value in process_tree(worker.pid).items()})
                if worker.poll() is not None:
                    break
                if time.monotonic() - started >= seconds:
                    return {'status': 'timeout', 'reason': f'Measurement exceeded watchdog limit of {seconds:.1f} s',
                            'watchdogSeconds': seconds, 'metrics': {}}
                if heartbeat:
                    heartbeat()
                time.sleep(.2)
            if response.is_file():
                return json.loads(response.read_text())
            return {'status': 'missing', 'reason': f'Measurement worker exited {worker.returncode}; see worker.log', 'metrics': {}}
        finally:
            # Only our descendants, and only if /proc still has the same birth time.
            for pid, birth in reversed(list(tracked.items())):
                try:
                    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
                    if int(fields[19]) == birth:
                        os.kill(pid, signal.SIGKILL)
                except (OSError, ValueError):
                    pass
            worker.wait(timeout=5)
