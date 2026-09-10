"""Private measurement process; communicates through local JSON artifacts."""
import ctypes
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

from suite import measure


def main():
    # PR_SET_CHILD_SUBREAPER: retain double-forked descendants for the watchdog.
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0):
        raise RuntimeError('Could not enable measurement subreaper')
    request = json.loads(Path(sys.argv[1]).read_text())
    try:
        result = measure(request['group'], request['command'], Path(request['fixture']),
                         request['renderer'], request['spec'], SimpleNamespace(**request['options']),
                         Path(request['artifact']), request['hz'])
    except subprocess.TimeoutExpired as error:
        result = {'status': 'timeout', 'reason': str(error), 'metrics': {}}
    except Exception as error:
        result = {'status': 'missing', 'reason': f'{type(error).__name__}: {error}', 'metrics': {}}
        if 'no-frames:' in str(error):
            result['proofFailure'] = {'kind': 'no-frames', 'capturedFrames': 0}
        elif 'already visible before stimulus' in str(error):
            result['proofFailure'] = {'kind': 'preexisting-text', 'capturedFrames': 0}
        elif 'Viewer exited before' in str(error):
            result['proofFailure'] = {'kind': 'program-exited', 'capturedFrames': 0}
    finally:
        from processes import process_tree
        for pid in reversed(list(process_tree(os.getpid()))):
            if pid != os.getpid():
                try:
                    state = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
                    if state == 'Z':
                        try:
                            os.waitpid(pid, os.WNOHANG)
                        except ChildProcessError:
                            pass
                        continue
                    os.kill(pid, signal.SIGKILL)
                except OSError as error:
                    # Activated helpers may have changed credentials. Keep the
                    # measurement and expose cleanup failures for inspection.
                    result.setdefault('cleanupErrors', []).append({'pid': pid, 'reason': str(error)})
    Path(sys.argv[2]).write_text(json.dumps(result) + '\n')


if __name__ == '__main__':
    main()
