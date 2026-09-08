"""Real GNOME/Mutter input for native window acceptance (local session only)."""
from gi.repository import Gio, GLib
import time


class NativeInput:
    def __init__(self):
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.destination = 'org.gnome.Mutter.RemoteDesktop'
        self.path = self.bus.call_sync(self.destination, '/org/gnome/Mutter/RemoteDesktop', self.destination,
                                      'CreateSession', None, None, Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
        # Associate one monitor stream solely to address absolute pointer coordinates.
        # Centering the test window prevents GNOME edge tiling from changing its
        # size halfway through a drag/resize assertion.
        session_id = self.bus.call_sync(self.destination,self.path,'org.freedesktop.DBus.Properties','Get',
            GLib.Variant('(ss)',(self.destination+'.Session','SessionId')),None,Gio.DBusCallFlags.NONE,-1,None).unpack()[0]
        display = self.bus.call_sync('org.gnome.Mutter.DisplayConfig','/org/gnome/Mutter/DisplayConfig','org.gnome.Mutter.DisplayConfig','GetCurrentState',None,None,Gio.DBusCallFlags.NONE,-1,None).unpack()
        primary = next(m for m in display[2] if m[4])
        monitor = primary[5][0][0]
        self.monitor_name = monitor
        physical = next(m for m in display[1] if m[0][0]==monitor)
        mode = next(m for m in physical[1] if m[6].get('is-current'))
        self.monitor_size = (mode[1]/primary[2],mode[2]/primary[2])
        self.monitor_origin = primary[:2]
        screencast='org.gnome.Mutter.ScreenCast'
        session=self.bus.call_sync(screencast,'/org/gnome/Mutter/ScreenCast',screencast,'CreateSession',
            GLib.Variant('(a{sv})',({'remote-desktop-session-id':GLib.Variant('s',session_id)},)),None,Gio.DBusCallFlags.NONE,-1,None).unpack()[0]
        self.stream=self.bus.call_sync(screencast,session,screencast+'.Session','RecordMonitor',
            GLib.Variant('(sa{sv})',(monitor,{'cursor-mode':GLib.Variant('u',0)})),None,Gio.DBusCallFlags.NONE,-1,None).unpack()[0]
        self.call('Start')

    def call(self, method, signature=None, values=None):
        return self.bus.call_sync(self.destination, self.path, self.destination + '.Session', method,
                                  GLib.Variant(signature, values) if signature else None,
                                  None, Gio.DBusCallFlags.NONE, -1, None)

    def keys(self, *symbols):
        for symbol in symbols: self.call('NotifyKeyboardKeysym', '(ub)', (symbol, True))
        for symbol in reversed(symbols): self.call('NotifyKeyboardKeysym', '(ub)', (symbol, False))
        time.sleep(.15)

    def move(self, dx, dy):
        self.call('NotifyPointerMotionRelative', '(dd)', (dx, dy))
        time.sleep(.15)

    def center(self):
        self.call('NotifyPointerMotionAbsolute','(sdd)',(self.stream,self.monitor_size[0]/2,self.monitor_size[1]/2))
        time.sleep(.2)

    def button(self, down):
        self.call('NotifyPointerButton', '(ib)', (272, down))
        time.sleep(.15)

    def focus(self, app):
        # Cycle existing windows, sending no document-editing keystrokes.
        for attempt in range(30):
            if app.js('return document.hasFocus()'): return
            self.call('NotifyKeyboardKeysym', '(ub)', (0xffe9, True))
            for _ in range(attempt + 1):
                self.keys(0xff09)  # Tab while Alt remains held; includes minimized windows.
            self.call('NotifyKeyboardKeysym', '(ub)', (0xffe9, False))
            time.sleep(.2)
        raise AssertionError('Desktop window switch did not focus Hashline')

    def close(self):
        self.call('Stop')
