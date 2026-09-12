"""Drive real scrolling in the native reader for a compositor frame-time run.

Replaces the WebDriver-based `scroll-profile.py`: the native application has no
script bridge, so the stimulus is real pointer input through
`org.gnome.Mutter.RemoteDesktop`, exactly the way a user's wheel reaches it.
That keeps the measurement external — nothing in the application is
instrumented, and nothing about the run depends on the toolkit
(docs/metrics.md).

The monotonic boundaries written to `--output` are what lets
`analyze-compositor.py` cut an interior window out of the surrounding Sysprof
trace, so start-up and shutdown frames never enter the statistics.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
import tempfile
import shutil
import signal
import re
from memory import PrivateBus, ISOLATION
from content import Capture, EXPECTED
from presentation import analyze

from gi.repository import Gio, GLib


def cpu_seconds(pid):
    """User plus system time of a process, in seconds."""
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(') ', 1)[1].split()
    except (OSError, IndexError):
        return None
    ticks = os.sysconf('SC_CLK_TCK')
    return (int(fields[11]) + int(fields[12])) / ticks

REMOTE = 'org.gnome.Mutter.RemoteDesktop'
SCREENCAST = 'org.gnome.Mutter.ScreenCast'


class Pointer:
    """A pointer on one monitor, addressed absolutely."""

    def __init__(self, connector):
        from session import connection
        self.bus = connection()
        self.path = self._call_object('CreateSession')
        session_id = self.bus.call_sync(
            REMOTE, self.path, 'org.freedesktop.DBus.Properties', 'Get',
            GLib.Variant('(ss)', (REMOTE + '.Session', 'SessionId')),
            None, Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
        # A screencast stream is required only to be able to name absolute
        # coordinates on a specific monitor.
        cast = self.bus.call_sync(
            SCREENCAST, '/org/gnome/Mutter/ScreenCast', SCREENCAST, 'CreateSession',
            GLib.Variant('(a{sv})', ({'remote-desktop-session-id': GLib.Variant('s', session_id)},)),
            None, Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
        self.stream = self.bus.call_sync(
            SCREENCAST, cast, SCREENCAST + '.Session', 'RecordMonitor',
            GLib.Variant('(sa{sv})', (connector, {'cursor-mode': GLib.Variant('u', 0)})),
            None, Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
        self.call('Start')
        if os.environ.get('HASHLINE_SESSION_KIND') == 'nested-headless':
            # Headless seats initially have no keyboard. Create it before the
            # stimulus; otherwise the first chord arrives before the client
            # receives wl_seat.capabilities and binds wl_keyboard.
            self.call('NotifyKeyboardKeysym', '(ub)', (65505, True))
            self.call('NotifyKeyboardKeysym', '(ub)', (65505, False))
            time.sleep(.05)

    def _call_object(self, method):
        return self.bus.call_sync(REMOTE, '/org/gnome/Mutter/RemoteDesktop', REMOTE,
                                  method, None, None, Gio.DBusCallFlags.NONE, -1, None).unpack()[0]

    def call(self, method, signature=None, values=None):
        return self.bus.call_sync(REMOTE, self.path, REMOTE + '.Session', method,
                                  GLib.Variant(signature, values) if signature else None,
                                  None, Gio.DBusCallFlags.NONE, -1, None)

    def to(self, x, y):
        self.call('NotifyPointerMotionAbsolute', '(sdd)', (self.stream, x, y))

    def scroll(self, dy):
        # Continuous axis deltas, the same shape a wheel or touchpad produces,
        # so the toolkit's own kinetic handling is exercised rather than
        # bypassed (docs/design.md).
        self.call('NotifyPointerAxis', '(ddu)', (0.0, dy, 0))

    def close(self):
        try:
            self.call('Stop')
        except GLib.Error:
            pass


def monitor_geometry(connector):
    from session import connection
    bus = connection()
    state = bus.call_sync('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig',
                          'org.gnome.Mutter.DisplayConfig', 'GetCurrentState', None, None,
                          Gio.DBusCallFlags.NONE, -1, None).unpack()
    physical = next(entry for entry in state[1] if entry[0][0] == connector)
    mode = next(mode for mode in physical[1] if mode[6].get('is-current'))
    logical = next(entry for entry in state[2] if entry[5][0][0] == connector)
    scale = logical[2]
    return mode[1] / scale, mode[2] / scale


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('fixture')
    parser.add_argument('--connector', default='DP-3')
    parser.add_argument('--refresh-hz', type=float, required=True)
    parser.add_argument('--seconds', type=float, default=12.0)
    parser.add_argument('--iterations', type=int, default=3,
                        help='scroll runs inside one capture, for repeats')
    parser.add_argument('--settle', type=float, default=4.0)
    parser.add_argument('--step', type=float, default=8.0)
    parser.add_argument('--reverse-after', type=float, default=2.0,
                        help='seconds before the scroll direction flips')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--content-proof', action='store_true')
    parser.add_argument('--schema-dir', type=Path, default=Path('target/schemas'))
    args = parser.parse_args()
    from storage import require_local_output
    if args.output is not None:
        try:
            require_local_output(args.output)
        except ValueError as error:
            parser.error(str(error))
    if args.output.exists():
        parser.error('Output exists; use a new path')

    # The reader is single-instance: launching it while another copy runs hands
    # the file over and the new process exits at once. Measuring that process
    # would report a clean, empty run, so it is refused outright.
    binary_name = Path(args.binary).name
    existing = subprocess.run(['pgrep', '-x', binary_name], capture_output=True, text=True)
    if existing.returncode == 0:
        parser.error(f'{binary_name} is already running (pids {existing.stdout.split()}); '
                     'a second launch would hand off instead of being measured')

    width, height = monitor_geometry(args.connector)
    # Into this process's environment, not only the child's. The window
    # rectangle comes from the Shell when it will answer an Eval and from
    # AT-SPI when it will not, which is every desktop except the private
    # nested one, and the AT-SPI path resolves the monitor from the
    # environment of whoever asks. Handing the connector only to the viewer
    # left it unresolvable, and the bare-metal scroll group died in the lookup
    # instead of measuring anything.
    os.environ['HASHLINE_MONITOR'] = args.connector
    environment = {**os.environ}
    if args.schema_dir.exists():
        environment['GSETTINGS_SCHEMA_DIR'] = str(args.schema_dir.resolve())
    record = {
        'binary': args.binary,
        'fixture': args.fixture,
        'connector': args.connector,
        'refreshHz': args.refresh_hz,
        'requestedSeconds': args.seconds,
        'stepPx': args.step,
        'reverseAfterSeconds': args.reverse_after,
        'method': 'Real Mutter RemoteDesktop axis events at the monitor centre; '
                  'monotonic boundaries bound an interior window in an external '
                  'compositor trace. Nothing in the application is instrumented.',
    }
    bus = PrivateBus()
    home = tempfile.mkdtemp(prefix='hashline-scroll-')
    environment.update(ISOLATION)
    environment['DBUS_SESSION_BUS_ADDRESS'] = bus.address
    for name in ('CONFIG', 'DATA', 'CACHE', 'STATE'):
        directory = Path(home) / name.lower()
        directory.mkdir()
        environment[f'XDG_{name}_HOME'] = str(directory)
    pointer_setup = Pointer(args.connector)
    pointer_setup.to(width / 2, height / 2)
    pointer_setup.close()
    capture = Capture(args.connector) if args.content_proof else None
    protocol = args.output.with_suffix('.wayland.log').open('w+')
    environment.update(WAYLAND_DEBUG='1', HASHLINE_BENCH_METADATA='1')
    launched = time.monotonic_ns()
    application = subprocess.Popen([args.binary, args.fixture], env=environment, start_new_session=True,
                                   stdout=subprocess.DEVNULL, stderr=protocol)
    pointer = None
    try:
        # The window has to exist and have settled before the interior window
        # opens, or start-up frames would be counted as scrolling.
        time.sleep(args.settle)
        if application.poll() is not None:
            record['error'] = 'application exited before scrolling (handed off?)'
            protocol.seek(0)
            record['stderr'] = protocol.read()[-2000:]
            args.output.write_text(json.dumps(record, indent=2) + '\n')
            return 1
        if capture:
            from session import window_bounds
            old_bus = os.environ.get('DBUS_SESSION_BUS_ADDRESS')
            os.environ['DBUS_SESSION_BUS_ADDRESS'] = bus.address
            try:
                capture.bounds = window_bounds(application.pid, capture.bus)
            finally:
                if old_bus is not None:
                    os.environ['DBUS_SESSION_BUS_ADDRESS'] = old_bus
                else:
                    os.environ.pop('DBUS_SESSION_BUS_ADDRESS', None)
            capture.close()
            record['contentProof'] = capture.proof(launched, EXPECTED[Path(args.fixture).stem], args.output.parent / 'content')
            if not record['contentProof']['contentVerified']:
                raise RuntimeError('No document text before scrolling')
            time.sleep(1)
        pointer = Pointer(args.connector)
        # Where the window sits cannot be read back: Wayland tells a client
        # nothing about its own position, and screenshots are not available on
        # every machine. So the window is *found* — a few events are sent at a
        # candidate point and the application's CPU time says whether they
        # landed. This also makes the run independent of how the compositor
        # chose to place or size the window.
        target = None
        probes = [(width / 2, height / 2)]
        for fraction_y in (0.5, 0.35, 0.65):
            for fraction_x in (0.5, 0.3, 0.7):
                probes.append((width * fraction_x, height * fraction_y))
        for x, y in probes:
            pointer.to(x, y)
            time.sleep(0.25)
            before = cpu_seconds(application.pid)
            for _ in range(12):
                pointer.scroll(args.step)
                time.sleep(1 / 120)
            time.sleep(0.25)
            after = cpu_seconds(application.pid)
            if before is not None and after is not None and after - before > 0.005:
                target = (x, y)
                break
        record['probedPoints'] = len(probes)
        if target is None:
            record['error'] = 'no pointer position reached the window'
            return 1
        record['pointer'] = {'x': target[0], 'y': target[1]}
        pointer.to(*target)
        time.sleep(0.5)

        # One capture holds several runs: the analyser cuts a ten-second
        # interior out of each, so repeats cost one display-mode change and one
        # trace instead of N of them.
        samples = []
        for iteration in range(1, args.iterations + 1):
            # Without this a run could report a clean trace while the events
            # never reached the window: an application that receives no input
            # spends no time on it.
            cpu_before = cpu_seconds(application.pid)
            interval = 1.0 / args.refresh_hz
            started_ns = time.monotonic_ns()
            started = time.monotonic()
            events = 0
            next_tick = started
            # The direction reverses periodically: a document scrolled to its
            # end has no further work to do, and a run that spends half its
            # time at the bottom would measure idleness rather than scrolling.
            while True:
                now = time.monotonic()
                elapsed = now - started
                if elapsed >= args.seconds:
                    break
                direction = 1.0 if int(elapsed / args.reverse_after) % 2 == 0 else -1.0
                pointer.scroll(args.step * direction)
                events += 1
                next_tick += interval
                delay = next_tick - time.monotonic()
                if delay > 0:
                    time.sleep(delay)
            ended_ns = time.monotonic_ns()
            cpu_after = cpu_seconds(application.pid)
            share = None
            if cpu_before is not None and cpu_after is not None:
                share = (cpu_after - cpu_before) / ((ended_ns - started_ns) / 1e9)
            samples.append({
                'fixture': args.fixture,
                'iteration': iteration,
                'commandStartMonotonicNs': started_ns,
                'observedEndMonotonicNs': ended_ns,
                'events': events,
                'appCpuShare': share,
                # A window that repainted for twelve seconds cannot have used
                # no CPU. A rate, not a total: a short run would fail an
                # absolute bound that a long one passes for the same behaviour.
                'reachedApplication': share is not None and share > 0.02,
                'alive': application.poll() is None,
            })
            time.sleep(1.0)
        record['raw'] = samples
        record['refreshHzDeclared'] = args.refresh_hz
        record['complete'] = bool(samples) and all(
            sample['alive'] and sample['events'] > 0 and sample['reachedApplication']
            for sample in samples)
    finally:
        if pointer is not None:
            pointer.close()
        try:
            os.killpg(application.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            application.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(application.pid, signal.SIGKILL)
            application.wait()
        if capture:
            capture.close()
        protocol.seek(0)
        trace = protocol.read()
        protocol.close()
        metadata = re.search(r'HASHLINE_BENCH renderer=(\S+) backend=(\S+)', trace)
        record['rendererObserved'] = metadata[1] if metadata else 'unknown'
        record['backendObserved'] = metadata[2] if metadata else 'unknown'
        try:
            record['applicationPresentation'] = analyze(trace, record.get('raw', []), args.refresh_hz)
            record['applicationPresentation']['contentVerified'] = bool(record.get('contentProof', {}).get('contentVerified'))
        except ValueError as error:
            record['applicationPresentation'] = {'applicationFramesVerified': False, 'reason': str(error)}
        bus.stop()
        shutil.rmtree(home, ignore_errors=True)
        args.output.write_text(json.dumps(record, indent=2) + '\n')
    return 0 if record.get('complete') else 1


if __name__ == '__main__':
    raise SystemExit(main())
