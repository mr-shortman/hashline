"""Capture GNOME Shell's Sysprof stream at the existing display mode.

DisplayConfig is read-only. This tool never switches or restores monitor modes.
--refresh-hz, when provided, asserts the existing rate; a mismatch is an error.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import gi
from gi.repository import Gio, GLib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('fixture')
    parser.add_argument('--refresh-hz', type=float, help='Assert current Hz without changing it')
    parser.add_argument('--connector', help='Monitor connector; defaults to primary')
    parser.add_argument('--content-proof', action='store_true')
    parser.add_argument('--iterations', type=int, default=3)
    parser.add_argument('--seconds', type=float, default=12.0)
    parser.add_argument('--output-prefix', type=Path, required=True)
    args = parser.parse_args()
    from storage import require_local_output
    if args.output_prefix is not None:
        try:
            require_local_output(args.output_prefix)
        except ValueError as error:
            parser.error(str(error))
    prefix = args.output_prefix
    state_file = Path(str(prefix) + '-display.json')
    capture_file = Path(str(prefix) + '.syscap')
    scroll_file = Path(str(prefix) + '-scroll.json')
    if any(p.exists() for p in [state_file, capture_file, scroll_file]):
        parser.error('Output exists; use a new prefix')
    prefix.parent.mkdir(parents=True, exist_ok=True)
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    def display(method, params=None):
        return connection.call_sync('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig',
                                    'org.gnome.Mutter.DisplayConfig', method, params, None,
                                    Gio.DBusCallFlags.NONE, 10000, None).unpack()
    def mode_map(state):
        return {spec[0]: next(mode[0] for mode in modes if mode[-1].get('is-current'))
                for spec, modes, _ in state[1] if any(m[-1].get('is-current') for m in modes)}
    before = display('GetCurrentState')
    original = mode_map(before)
    primary = args.connector or next(row for row in before[2] if row[4])[5][0][0]
    if primary not in original:
        parser.error('Requested connector has no current mode')
    available = next(modes for spec, modes, _ in before[1] if spec[0] == primary)
    chosen = next(mode for mode in available if mode[0] == original[primary])
    if args.refresh_hz is not None and abs(chosen[3] - args.refresh_hz) >= 1:
        parser.error(f'Current refresh is {chosen[3]:.3f} Hz; requested {args.refresh_hz}. No display change performed.')
    output = {'primaryConnector': primary, 'requestedRefreshHz': args.refresh_hz,
              'observedRefreshHz': chosen[3], 'selectedMode': chosen[0],
              'before': before, 'complete': False, 'displayConfigurationChanged': False,
              'method': 'GNOME Shell Sysprof capture at the existing monitor mode; DisplayConfig read-only.'}
    started = False
    fd = None
    try:
        output['during'] = display('GetCurrentState')
        fd = os.open(capture_file, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        fds = Gio.UnixFDList.new()
        index = fds.append(fd)
        connection.call_with_unix_fd_list_sync('org.gnome.Shell', '/org/gnome/Sysprof3/Profiler',
            'org.gnome.Sysprof3.Profiler', 'Start', GLib.Variant('(a{sv}h)', ({}, index)),
            None, Gio.DBusCallFlags.NONE, 10000, fds, None)
        started = True
        result = subprocess.run([sys.executable, 'benchmarks/scroll-native.py', args.binary,
            args.fixture, '--connector', primary, '--refresh-hz', str(chosen[3]),
            '--seconds', str(args.seconds), '--iterations', str(args.iterations), '--output', str(scroll_file),
            *(['--content-proof'] if args.content_proof else [])])
        output['scrollExitCode'] = result.returncode
        output['complete'] = result.returncode == 0
    except Exception as error:
        output['error'] = str(error)
        raise
    finally:
        if started:
            try:
                connection.call_sync('org.gnome.Shell', '/org/gnome/Sysprof3/Profiler',
                    'org.gnome.Sysprof3.Profiler', 'Stop', None, None, Gio.DBusCallFlags.NONE, 10000, None)
            except Exception as error:
                output['stopError'] = str(error)
        if fd is not None:
            os.close(fd)
        try:
            output['after'] = display('GetCurrentState')
            output['modeUnchanged'] = mode_map(output['after']).get(primary) == original[primary]
        except Exception as error:
            output['displayReadError'] = str(error)
            output['modeUnchanged'] = None
        state_file.write_text(json.dumps(output, indent=2) + '\n')
    return 0 if output['complete'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
