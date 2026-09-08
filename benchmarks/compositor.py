"""Capture GNOME Shell's Sysprof stream during real 60/120 Hz scroll runs.

Uses the current desktop bus for the compositor and a separate bus for the app.
A requested display-mode change is temporary and restored in finally.
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
    parser.add_argument('--refresh-hz', type=float, required=True)
    parser.add_argument('--test-bus-file', type=Path, required=True)
    parser.add_argument('--driver-url', default='http://127.0.0.1:4545')
    parser.add_argument('--output-prefix', type=Path, required=True)
    args = parser.parse_args()
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
    def apply(state, modes):
        logical = [(x, y, scale, transform, primary,
                    [(spec[0], modes[spec[0]], {}) for spec in monitors])
                   for x, y, scale, transform, primary, monitors, _ in state[2]]
        properties = {'layout-mode': GLib.Variant('u', state[3]['layout-mode'])} if 'layout-mode' in state[3] else {}
        display('ApplyMonitorsConfig', GLib.Variant('(uua(iiduba(ssa{sv}))a{sv})', (state[0], 1, logical, properties)))
    before = display('GetCurrentState')
    original = mode_map(before)
    primary = next(row for row in before[2] if row[4])[5][0][0]
    available = next(modes for spec, modes, _ in before[1] if spec[0] == primary)
    candidates = [mode for mode in available if abs(mode[3] - args.refresh_hz) < 1 and mode[1:3] == next(m[1:3] for m in available if m[0] == original[primary])]
    if not candidates:
        parser.error('Requested refresh rate is unavailable at the current resolution')
    chosen = min(candidates, key=lambda mode: abs(mode[3] - args.refresh_hz))
    output = {'primaryConnector': primary, 'requestedRefreshHz': args.refresh_hz,
              'selectedMode': chosen[0], 'before': before, 'complete': False,
              'method': 'GNOME Shell org.gnome.Sysprof3.Profiler; temporary primary-monitor mode with finally restoration. Trace includes the compositor; app frame attribution must be verified during analysis.'}
    changed = chosen[0] != original[primary]
    started = False
    fd = None
    try:
        if changed:
            apply(before, {**original, primary: chosen[0]})
        output['during'] = display('GetCurrentState')
        fd = os.open(capture_file, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        fds = Gio.UnixFDList.new()
        index = fds.append(fd)
        connection.call_with_unix_fd_list_sync('org.gnome.Shell', '/org/gnome/Sysprof3/Profiler',
            'org.gnome.Sysprof3.Profiler', 'Start', GLib.Variant('(a{sv}h)', ({}, index)),
            None, Gio.DBusCallFlags.NONE, 10000, fds, None)
        started = True
        env = {**os.environ, 'DBUS_SESSION_BUS_ADDRESS': args.test_bus_file.read_text().strip(),
               'HASHLINE_WEBDRIVER_URL': args.driver_url}
        result = subprocess.run([sys.executable, 'benchmarks/scroll-profile.py', args.binary,
            '--refresh-hz', str(chosen[3]), '--output', str(scroll_file)], env=env)
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
            if changed:
                current = display('GetCurrentState')
                modes = mode_map(current)
                # Restore only our own refresh change, respecting unrelated user changes.
                if modes.get(primary) == chosen[0]:
                    apply(current, {**modes, primary: original[primary]})
            output['after'] = display('GetCurrentState')
            output['restored'] = mode_map(output['after']).get(primary) == original[primary]
        except Exception as error:
            output['restoreError'] = str(error)
            output['restored'] = False
        state_file.write_text(json.dumps(output, indent=2) + '\n')


if __name__ == '__main__':
    main()
