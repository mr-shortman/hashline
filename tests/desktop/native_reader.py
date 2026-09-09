"""Native GTK smoke test over AT-SPI; use an isolated dbus-run-session.

Requires python3-gi and gir1.2-atspi-2.0, plus an X11/Wayland display.
Broadway does not expose GTK's AT-SPI backend.
"""
import gi, subprocess, time, tempfile, sys, os, signal
from pathlib import Path
BINARY = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/hashline').resolve())
DESKTOP = '--desktop' in sys.argv[2:]
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, Gio, GLib

if DESKTOP:
    launcher = Gio.DesktopAppInfo.new('de.kalendium.Hashline.desktop')
    assert launcher is not None, 'Installed desktop entry missing'
    assert launcher.get_name() == 'Hashline'
    assert launcher.get_generic_name() == 'Markdown Viewer'
    assert launcher.get_commandline() == 'hashline %f'
    assert launcher.get_icon().to_string() == 'de.kalendium.Hashline'
    for content_type in ('text/markdown', 'text/x-markdown'):
        assert any(app.get_id() == launcher.get_id()
                   for app in Gio.AppInfo.get_all_for_type(content_type)), content_type
    assert Gio.SettingsSchemaSource.get_default().lookup('de.kalendium.Hashline', True)

def descendants(node):
    yield node
    for i in range(node.get_child_count()):
        try: yield from descendants(node.get_child_at_index(i))
        except Exception: pass

def document(expected):
    for _ in range(60):
        for node in descendants(Atspi.get_desktop(0)):
            if node.get_role() in (Atspi.Role.DOCUMENT_FRAME, Atspi.Role.DOCUMENT_TEXT):
                text = node.get_text_iface()
                if text and expected in Atspi.Text.get_text(text, 0, -1): return node, text
        time.sleep(.1)
    print([(n.get_role_name(), n.get_name()) for n in descendants(Atspi.get_desktop(0))], flush=True)
    raise AssertionError('No accessible document: '+expected)

with tempfile.TemporaryDirectory(prefix='hashline-atspi-') as tmp:
    first=Path(tmp)/'erste Datei.md'; second=Path(tmp)/'zweite.md'
    first.write_text('# Grüße 🌍\n\nÄpfel und Öl.\n'); second.write_text('# Zweite Datei\n\nNeuer Text.\n')
    app=subprocess.Popen([BINARY,str(first)])
    try:
        node,text=document('Grüße 🌍')
        assert Atspi.Text.get_text(text, 4,7)=='e 🌍', repr(Atspi.Text.get_text(text, 4,7))
        assert text.get_character_count()==len('Grüße 🌍\n\nÄpfel und Öl.'), text.get_character_count()
        print('AT-SPI Document role, Text interface, Unicode offsets: OK',flush=True)
        subprocess.run([BINARY,str(second),str(first)],check=True,timeout=5)
        node,text=document('Zweite Datei')
        applications=[n for n in descendants(Atspi.get_desktop(0)) if n.get_role()==Atspi.Role.APPLICATION and n.get_process_id()==app.pid]
        assert len(applications)==1, len(applications)
        frames=[n for n in descendants(applications[0]) if n.get_role()==Atspi.Role.FRAME]
        assert len(frames)==1,len(frames)
        time.sleep(.4)
        labels=[n.get_name() for n in descendants(applications[0])]
        assert any('erste Datei wurde gewählt' in label for label in labels), labels
        print('Second process reuses one window; multi-file notice visible: OK',flush=True)

        other = Path(tmp) / 'anderes Verzeichnis'
        other.mkdir()
        relative = other / '-Grüße %20 #.md'
        relative.write_text('# Relativer Aufruf\n\nAufruferverzeichnis und Sonderzeichen.\n')
        subprocess.run([BINARY, '--', relative.name], cwd=other, check=True, timeout=5)
        document('Relativer Aufruf')
        subprocess.run([BINARY], cwd=other, check=True, timeout=5)
        document('Relativer Aufruf')
        print('Caller-relative paths, Unicode, spaces, %, #, -- and empty activation: OK', flush=True)

        if DESKTOP:
            # GIO's desktop launch is the same association/Exec expansion path
            # used by file managers. % in a literal path must not become an escape.
            desktop_file = other / 'Dateimanager Grüße %20 #.md'
            desktop_file.write_text('# Desktop-Aufruf\n\nInstalliertes Paket.\n')
            assert launcher.launch([Gio.File.new_for_path(str(desktop_file))], None)
            node, _ = document('Desktop-Aufruf')
            assert node.get_process_id() == app.pid
            frames = [n for n in descendants(applications[0]) if n.get_role() == Atspi.Role.FRAME]
            assert len(frames) == 1, len(frames)
            print('Installed desktop/MIME launcher forwards to the same process and window: OK', flush=True)
    finally:
        app.terminate(); app.wait(timeout=5)

    if DESKTOP:
        launched = []
        assert launcher.launch_uris_as_manager(
            [first.as_uri()], None, GLib.SpawnFlags.DO_NOT_REAP_CHILD,
            None, None, lambda info, pid, data: launched.append(pid), None)
        assert len(launched) == 1, launched
        pid = launched[0]
        try:
            node, _ = document('Grüße 🌍')
            assert node.get_process_id() == pid
            print('Cold start through the installed desktop entry: OK', flush=True)
        finally:
            os.kill(pid, signal.SIGTERM)
            for _ in range(50):
                if os.waitpid(pid, os.WNOHANG)[0]:
                    break
                time.sleep(.1)
            else:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
