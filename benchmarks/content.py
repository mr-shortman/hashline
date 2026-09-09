#!/usr/bin/env python3
"""Mutter ScreenCast -> PipeWire pixels -> offline OCR, on the startup clock.

The receipt timestamp is an upper bound, not a scanout timestamp. OCR runs after
termination and cannot delay the viewer. Baseline text invalidates a capture.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time
import zlib

ROOT = Path(__file__).resolve().parent
SCREENCAST = 'org.gnome.Mutter.ScreenCast'

# Phrases must be readable as one block and unique to their own fixture, so a
# frame showing some other document can never be accepted as this one.
EXPECTED = {
    'small': ['Lesbarer Text mit', 'Dieser Absatz'],
    'medium': ['Lesbarer Text mit', 'Dieser Absatz'],
    'large': ['Lesbarer Text mit', 'Dieser Absatz'],
    'long-line': ['abcdefghijabcdefghij'],
    'deep-list': ['Tiefe Liste', 'Ebene 1'],
    'wide-table': ['Breite Tabelle', 'Inhalt'],
    'many-blocks': ['Kleine', 'Absatz'],
    'many-images': ['many-images', 'Bild 0'],
    'large-images': ['large-images', 'Bild 0'],
    'large-code': ['long code line'],
}


def normalize(text):
    return ' '.join(''.join(c.lower() if c.isalnum() else ' ' for c in text).split())


def contains(text, expected):
    return all(normalize(phrase) in normalize(text) for phrase in expected)


def engine():
    """The exact OCR build a content proof was read with, for the record."""
    binary = Path(shutil.which('tesseract') or ROOT / '.provision/ocr/root/usr/bin/tesseract')
    if not binary.is_file():
        return {'binary': str(binary), 'available': False,
                'reason': 'No tesseract for the content proof; run: python3 benchmarks/provision.py --tools'}
    return {'binary': str(binary), 'available': True, 'pageSegmentation': 6, 'language': 'eng',
            'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}


def ocr(frame):
    tool = engine()
    if not tool['available']:
        raise RuntimeError(tool['reason'])
    binary = tool['binary']
    env = dict(os.environ, OMP_THREAD_LIMIT='1')
    data = ROOT / '.provision/ocr/root/usr/share/tesseract-ocr/5/tessdata'
    if data.exists():
        env['TESSDATA_PREFIX'] = str(data)
    # Page mode 6 reads the document as one block. Sparse mode 11 lost whole
    # words on 11 of 25 recorded desktop frames whose text was plainly there.
    result = subprocess.run([binary, 'stdin', 'stdout', '-l', 'eng', '--psm', '6'],
                            input=frame, capture_output=True, env=env, timeout=60)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace'))
    return result.stdout.decode(errors='replace')


class Capture:
    def __init__(self, connector):
        import gi
        gi.require_version('Gst', '1.0')
        from gi.repository import Gio, GLib, Gst
        Gst.init(None)
        self.Gst, self.GLib, self.Gio = Gst, GLib, Gio
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.frames = []
        self.error = None
        self.lock = threading.Lock()
        self.pipeline = None
        self.session = None
        self.subscription = None
        self.total_bytes = 0
        self.last_hash = None
        self.loop = GLib.MainLoop()
        self.thread = threading.Thread(target=self.loop.run, daemon=True)
        self.thread.start()
        try:
            self.session = self.call('/org/gnome/Mutter/ScreenCast', SCREENCAST,
                                     'CreateSession', '(a{sv})', ({},))[0]
            self.stream = self.call(self.session, SCREENCAST + '.Session', 'RecordMonitor',
                                   '(sa{sv})', (connector, {'cursor-mode': GLib.Variant('u', 0)}))[0]
            self.subscription = self.bus.signal_subscribe(SCREENCAST, SCREENCAST + '.Stream',
                'PipeWireStreamAdded', self.stream, None, Gio.DBusSignalFlags.NONE, self.node_added)
            self.call(self.session, SCREENCAST + '.Session', 'Start')
            deadline = time.monotonic() + 10
            while not self.frames and not self.error and time.monotonic() < deadline:
                time.sleep(.01)
            if not self.frames:
                raise RuntimeError(self.error or 'ScreenCast produced no baseline frame in 10 s')
        except BaseException:
            self.close()
            raise

    def call(self, path, interface, method, signature=None, values=None):
        return self.bus.call_sync(SCREENCAST, path, interface, method,
            self.GLib.Variant(signature, values) if signature else None, None,
            self.Gio.DBusCallFlags.NONE, 10000, None).unpack()

    def node_added(self, connection, sender, path, interface, signal, params):
        try:
            node = params.unpack()[0]
            self.pipeline = self.Gst.parse_launch(
                f'pipewiresrc path={node} do-timestamp=true ! videoconvert ! '
                'video/x-raw,format=RGB ! appsink name=frames emit-signals=true sync=false max-buffers=0 drop=false')
            self.pipeline.get_by_name('frames').connect('new-sample', self.new_frame)
            self.pipeline.set_state(self.Gst.State.PLAYING)
        except Exception as error:
            self.error = str(error)

    def new_frame(self, sink):
        sample = sink.emit('pull-sample')
        received = time.monotonic_ns()
        buffer = sample.get_buffer()
        caps = sample.get_caps().get_structure(0)
        width, height = caps.get_value('width'), caps.get_value('height')
        ok, mapped = buffer.map(self.Gst.MapFlags.READ)
        if not ok:
            self.error = 'Unreadable PipeWire buffer'
            return self.Gst.FlowReturn.ERROR
        try:
            # RGB rows are padded to four-byte boundaries by GStreamer.
            stride = (width * 3 + 3) & ~3
            data = bytes(mapped.data)
            if len(data) != stride * height:
                self.error = 'Unexpected RGB stride'
                return self.Gst.FlowReturn.ERROR
            digest = hashlib.sha256(data).hexdigest()
            if digest != self.last_hash:
                pixels = b''.join(data[y * stride:y * stride + width * 3] for y in range(height))
                ppm = f'P6\n{width} {height}\n255\n'.encode() + pixels
                packed = zlib.compress(ppm, 1)
                with self.lock:
                    self.total_bytes += len(packed)
                    if self.total_bytes > 512 * 1024 * 1024:
                        self.error = 'Capture exceeds 512 MiB; no frames silently dropped'
                        return self.Gst.FlowReturn.ERROR
                    self.frames.append({'receivedMonotonicNs': received, 'ptsNs': buffer.pts,
                                        'sha256': hashlib.sha256(ppm).hexdigest(), 'packed': packed})
                self.last_hash = digest
        finally:
            buffer.unmap(mapped)
        return self.Gst.FlowReturn.OK

    def close(self):
        if self.pipeline:
            self.pipeline.set_state(self.Gst.State.NULL)
        if self.session:
            try:
                self.call(self.session, SCREENCAST + '.Session', 'Stop')
            except Exception:
                pass
        if self.subscription:
            self.bus.signal_unsubscribe(self.subscription)
        self.loop.quit()
        self.thread.join(timeout=5)

    def clear(self, expected, timeout=15):
        """Wait until the document is *not* on screen, then start from there.

        A viewer from the previous row can still be painted when this capture
        opens. Waiting for the desktop is honest; accepting the leftover as a
        result would be a fast measurement of the wrong window.
        """
        deadline = time.monotonic() + timeout
        while True:
            if self.error:
                raise RuntimeError(self.error)
            with self.lock:
                index, frame = len(self.frames) - 1, self.frames[-1]
            if not contains(ocr(zlib.decompress(frame['packed'])), expected):
                with self.lock:
                    self.frames = self.frames[index:]
                return frame
            if time.monotonic() >= deadline:
                raise RuntimeError(f'Expected document text still on screen after {timeout:g} s; '
                                   'no baseline without it')
            time.sleep(.5)

    def proof(self, started_ns, expected, directory):
        if self.error:
            raise RuntimeError(self.error)
        evidence = []
        first = None
        baseline = [frame for frame in self.frames if frame['receivedMonotonicNs'] < started_ns]
        if not baseline:
            raise RuntimeError('No pre-stimulus baseline')
        # Every baseline is checked, so pre-existing matching text cannot count.
        for frame in self.frames:
            pixels = zlib.decompress(frame['packed'])
            text = ocr(pixels)
            matched = contains(text, expected)
            record = {k: v for k, v in frame.items() if k != 'packed'}
            record.update(matched=matched, delayMs=(frame['receivedMonotonicNs'] - started_ns) / 1e6)
            evidence.append(record)
            if matched and record['delayMs'] < 0:
                raise RuntimeError('Expected document text already visible before stimulus')
            if matched and first is None:
                first = record
                directory.mkdir(parents=True, exist_ok=True)
                (directory / 'first-readable.ppm').write_bytes(pixels)
                (directory / 'first-readable.txt').write_text(text)
                break
        result = {'status': 'ok' if first else 'missing', 'contentVerified': first is not None,
                  'expected': expected, 'frames': evidence, 'capturedUniqueFrames': len(self.frames),
                  'method': 'Mutter monitor ScreenCast, PipeWire RGB; offline OCR. Receipt time is a conservative visibility upper bound including capture latency; not a presentation timestamp.',
                  'evidence': str(directory / 'first-readable.ppm') if first else None}
        if first:
            result['readableUpperMs'] = first['delayMs']
        else:
            directory.mkdir(parents=True, exist_ok=True)
            (directory / 'last-frame.ppm').write_bytes(pixels)
            (directory / 'last-frame.txt').write_text(text)
            result['reason'] = 'No captured frame contains the expected document text'
        return result
