"""Capture one actual compositor frame through Mutter/PipeWire for visual review.

This supplements WebDriver's pre-compositor WebView screenshots. The caller must
keep the test window maximized and retain only the monitor showing that window.
"""
from gi.repository import Gio, GLib
from pathlib import Path
import subprocess
import time


def capture(monitor, output):
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    destination = 'org.gnome.Mutter.ScreenCast'
    def call(path, interface, method, signature=None, values=None):
        return bus.call_sync(destination,path,interface,method,GLib.Variant(signature,values) if signature else None,None,Gio.DBusCallFlags.NONE,-1,None).unpack()
    session = call('/org/gnome/Mutter/ScreenCast',destination,'CreateSession','(a{sv})',({},))[0]
    stream = call(session,destination+'.Session','RecordMonitor','(sa{sv})',(monitor,{'cursor-mode':GLib.Variant('u',0)}))[0]
    node = []
    subscription = bus.signal_subscribe(destination,destination+'.Stream','PipeWireStreamAdded',stream,None,Gio.DBusSignalFlags.NONE,lambda *args:node.append(args[5].unpack()[0]))
    try:
        call(session,destination+'.Session','Start')
        deadline=time.monotonic()+10
        context=GLib.MainContext.default()
        while not node and time.monotonic()<deadline:
            while context.pending(): context.iteration(False)
            time.sleep(.01)
        assert node, 'No PipeWire node announced'
        subprocess.run(['gst-launch-1.0','-q','pipewiresrc',f'path={node[0]}','num-buffers=1','!','videoconvert','!','pngenc','!','filesink',f'location={Path(output).resolve()}'],check=True,timeout=15)
    finally:
        bus.signal_unsubscribe(subscription)
        call(session,destination+'.Session','Stop')
