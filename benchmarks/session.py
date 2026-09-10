"""Dedicated headless Mutter session and read-only desktop/window observations.

Mutter 50+: --wayland --headless --virtual-monitor; older releases also expose
--nested. No options change the host compositor, monitor mode or settings.
"""
import contextlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

from memory import PrivateBus


def connection(address=None):
    from gi.repository import Gio
    return Gio.DBusConnection.new_for_address_sync(address or os.environ['DBUS_SESSION_BUS_ADDRESS'],
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)


def shell_eval(code, bus=None):
    from gi.repository import Gio, GLib
    bus = bus or connection()
    ok, value = bus.call_sync('org.gnome.Shell', '/org/gnome/Shell', 'org.gnome.Shell', 'Eval',
        GLib.Variant('(s)', (code,)), None, Gio.DBusCallFlags.NONE, 3000, None).unpack()
    if not ok:
        raise RuntimeError('Shell window geometry unavailable: ' + value)
    return json.loads(value)


def window_bounds(pid, bus):
    """Read the actual target window rectangle, never infer it from screen size."""
    code = """(() => {
        const w = global.get_window_actors().map(a => a.meta_window)
            .find(w => w.get_pid() === PID && !w.is_override_redirect());
        if (!w) return null;
        const r = w.get_frame_rect();
        const m = global.display.get_monitor_geometry(w.get_monitor());
        return {x:r.x-m.x, y:r.y-m.y, width:r.width, height:r.height,
                monitor:w.get_monitor(), scale:global.display.get_monitor_scale(w.get_monitor()), mapped:!!w.get_compositor_private()?.is_visible()};
    })()""".replace('PID', str(int(pid)))
    try:
        value = shell_eval(code, bus)
        if value and value['mapped']:
            return value
    except RuntimeError:
        pass
    # Accessible clients expose a screen rectangle even when host Shell Eval
    # is disabled. This helper uses the app's private accessibility bus.
    helper = """
import gi, json, sys
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi
for app in Atspi.get_desktop(0):
    if app.get_process_id() == int(sys.argv[1]):
        for win in app:
            if win.get_state_set().contains(Atspi.StateType.SHOWING):
                r = win.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
                if r.width > 0 and r.height > 0:
                    print(json.dumps(dict(x=r.x,y=r.y,width=r.width,height=r.height,mapped=True)))
                    sys.exit(0)
sys.exit(1)
"""
    env = dict(os.environ, GTK_A11Y='atspi')
    env.pop('NO_AT_BRIDGE', None)
    result = subprocess.run(['/usr/bin/python3', '-c', helper, str(pid)], capture_output=True, text=True, env=env, timeout=4)
    if result.returncode == 0:
        value = json.loads(result.stdout)
        # AT-SPI screen coordinates refer to the entire logical desktop.
        from gi.repository import Gio
        state = bus.call_sync('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig',
            'org.gnome.Mutter.DisplayConfig', 'GetCurrentState', None, None, Gio.DBusCallFlags.NONE, 3000, None).unpack()
        connector = os.environ.get('HASHLINE_MONITOR')
        logical = next((r for r in state[2] if any(m[0] == connector for m in r[5])), None)
        if logical is None:
            raise RuntimeError(f'No logical monitor carries connector {connector!r}; the window '
                               'rectangle has no monitor to be placed on')
        value['x'] -= logical[0]
        value['y'] -= logical[1]
        value['scale'] = logical[2]
        # Only if it lands on the monitor being captured. Wayland tells a
        # client nothing about where its window is, so on a desktop whose
        # Shell will not answer an Eval, AT-SPI answers with the window's own
        # origin instead of the desktop's: a maximized window on a three
        # monitor desktop came back as (0, 0), which after the shift is a
        # rectangle 1080 pixels above the captured monitor. Cropping to that
        # is worse than not cropping, and the proof already reads the whole
        # monitor when it is given no rectangle.
        physical = next((m for m in state[1] if m[0][0] == connector), None)
        mode = next((m for m in physical[1] if m[6].get('is-current')), None) if physical else None
        if mode is None or value['x'] < 0 or value['y'] < 0 \
                or value['x'] + value['width'] > mode[1] or value['y'] + value['height'] > mode[2]:
            return None
        return value
    return None


def quiet_check(seconds):
    """Observe absence of input, not a promise that the host remains idle."""
    try:
        from gi.repository import Gio
        bus = connection()
        def idle():
            return bus.call_sync('org.gnome.Mutter.IdleMonitor', '/org/gnome/Mutter/IdleMonitor/Core',
                'org.gnome.Mutter.IdleMonitor', 'GetIdletime', None, None,
                Gio.DBusCallFlags.NONE, 3000, None).unpack()[0]
        before = idle()
        time.sleep(seconds)
        after = idle()
        return {'verified': after >= (seconds * 1000) and after >= before + seconds * 1000 - 100,
                'beforeIdleMs': before, 'afterIdleMs': after, 'observationSeconds': seconds,
                'method': 'Mutter input-idle counter observed before and after preflight; no guarantee of later inactivity'}
    except Exception as error:
        return {'verified': False, 'reason': str(error)}


class Session:
    def __init__(self, kind, resolution, directory, enabled=True):
        self.kind, self.resolution, self.directory, self.enabled = kind, resolution, directory, enabled
        self.connector = None
        self.stack = contextlib.ExitStack()

    def __enter__(self):
        if self.kind == 'bare-metal' or not self.enabled:
            return self
        if not re.fullmatch(r'[1-9]\d{2,3}x[1-9]\d{2,3}', self.resolution):
            raise ValueError('Resolution must be WIDTHxHEIGHT, e.g. 1920x1080')
        old = dict(os.environ)
        self.stack.callback(lambda: (os.environ.clear(), os.environ.update(old)))
        try:
            home = Path(self.stack.enter_context(tempfile.TemporaryDirectory(prefix='hashline-session-')))
            bus = PrivateBus()
            self.stack.callback(bus.stop)
            display = f'hashline-bench-{os.getpid()}'
            env = dict(os.environ, DBUS_SESSION_BUS_ADDRESS=bus.address, WAYLAND_DISPLAY=display,
                       GSETTINGS_BACKEND='memory', GNOME_SHELL_SLOWDOWN_FACTOR='1')
            env.pop('DISPLAY', None)
            for name in ('CONFIG', 'DATA', 'CACHE', 'STATE'):
                path = home / name.lower(); path.mkdir()
                env[f'XDG_{name}_HOME'] = str(path)
            log = self.stack.enter_context((self.directory / 'session.log').open('a'))
            command = ['gnome-shell', '--wayland', '--headless', '--no-x11', '--virtual-monitor', self.resolution,
                       '--wayland-display', display, '--unsafe-mode']
            process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            def stop():
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL); process.wait()
                except ProcessLookupError:
                    pass
            self.stack.callback(stop)
            os.environ.update(env)
            # Separate gdbus processes avoid a cached desktop Gio session connection.
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f'Nested compositor exited {process.returncode}; see session.log')
                probe = subprocess.run(['gdbus', 'call', '--session', '--dest', 'org.gnome.Shell',
                    '--object-path', '/org/gnome/Shell', '--method', 'org.gnome.Shell.Eval',
                    'true'], capture_output=True, text=True, timeout=3)
                if probe.returncode == 0 and probe.stdout.startswith('(true,'):
                    break
                time.sleep(.2)
            else:
                raise RuntimeError('Nested compositor did not become ready; see session.log')
            # UnsafeMode belongs only to this private compositor; host settings stay untouched.
            shell_eval('Main.overview.hide(); true')
            from display import current_mode
            mode = current_mode()
            self.connector = mode['connector']
            width, height = map(int, self.resolution.split('x'))
            if (mode['width'], mode['height']) != (width, height):
                raise RuntimeError('Nested display did not use requested fixed resolution')
            os.environ['HASHLINE_SESSION_KIND'] = self.kind
            return self
        except BaseException:
            self.stack.close()
            raise

    def __exit__(self, *exc):
        self.stack.close()
