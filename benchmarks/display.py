"""Read-only display conditions shared by the full suite and scroll driver."""


def current_mode(connector=None):
    from gi.repository import Gio
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    state = bus.call_sync('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig',
                         'org.gnome.Mutter.DisplayConfig', 'GetCurrentState', None, None,
                         Gio.DBusCallFlags.NONE, 10000, None).unpack()
    connector = connector or next(row for row in state[2] if row[4])[5][0][0]
    modes = next((modes for spec, modes, _ in state[1] if spec[0] == connector), [])
    mode = next((mode for mode in modes if mode[-1].get('is-current')), None)
    if mode is None:
        raise RuntimeError(f'No active mode for connector {connector}')
    return {'connector': connector, 'mode': mode[0], 'width': mode[1], 'height': mode[2], 'refreshHz': mode[3]}


def verify_rate(requested, connector=None):
    mode = current_mode(connector)
    if abs(mode['refreshHz'] - requested) >= 1:
        raise RuntimeError(f"Requested {requested:g} Hz, but {mode['connector']} is at {mode['refreshHz']:.3f} Hz. No display configuration changed.")
    return mode
