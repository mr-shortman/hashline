"""Native GTK smoke test over AT-SPI; use an isolated dbus-run-session.

Requires python3-gi and gir1.2-atspi-2.0, plus an X11/Wayland display.
Broadway does not expose GTK's AT-SPI backend.
"""
import gi, subprocess, time, tempfile, sys
from pathlib import Path
BINARY = sys.argv[1] if len(sys.argv) > 1 else 'target/debug/hashline'
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

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
        applications=[n for n in descendants(Atspi.get_desktop(0)) if n.get_role()==Atspi.Role.APPLICATION and n.get_name()=='hashline']
        assert len(applications)==1, len(applications)
        frames=[n for n in descendants(applications[0]) if n.get_role()==Atspi.Role.FRAME]
        assert len(frames)==1,len(frames)
        time.sleep(.4)
        labels=[n.get_name() for n in descendants(applications[0])]
        assert any('erste Datei wurde gewählt' in label for label in labels), labels
        print('Second process reuses one window; multi-file notice visible: OK',flush=True)
    finally:
        app.terminate(); app.wait(timeout=5)
