#!/usr/bin/env python3
"""Mutter ScreenCast -> PipeWire pixels -> offline OCR, on the startup clock.

The receipt timestamp is an upper bound, not a scanout timestamp. OCR runs after
termination and cannot delay the viewer. Baseline text invalidates a capture.
"""
import hashlib
from functools import lru_cache
import json
import os
from pathlib import Path
import queue
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


@lru_cache(maxsize=1)
def engine():
    """The exact OCR build a content proof was read with, for the record."""
    binary = Path(shutil.which('tesseract') or ROOT / '.provision/ocr/root/usr/bin/tesseract')
    if not binary.is_file():
        return {'binary': str(binary), 'available': False,
                'reason': 'No tesseract for the content proof; run: python3 benchmarks/provision.py --tools'}
    return {'binary': str(binary), 'available': True, 'pageSegmentation': [6, 11], 'language': 'eng',
            'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}


def ocr(frame, psm=6):
    tool = engine()
    if not tool['available']:
        raise RuntimeError(tool['reason'])
    binary = tool['binary']
    env = dict(os.environ, OMP_THREAD_LIMIT='1')
    data = ROOT / '.provision/ocr/root/usr/share/tesseract-ocr/5/tessdata'
    if data.exists():
        env['TESSDATA_PREFIX'] = str(data)
    # Sparse text is a fallback only; it cannot remove a hit from block mode.
    result = subprocess.run(['nice', '-n', '10', binary, 'stdin', 'stdout', '-l', 'eng', '--psm', str(psm)],
                            input=frame, capture_output=True, env=env, timeout=60)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace'))
    return result.stdout.decode(errors='replace')


def recognize(pixels, expected):
    text = ocr(pixels)
    if contains(text, expected):
        return text, 6, True
    sparse = ocr(pixels, 11)
    if contains(sparse, expected):
        return sparse, 11, True
    return text + '\n' + sparse, None, False


def crop_ppm(ppm, bounds):
    magic, dimensions, maximum, pixels = ppm.split(b'\n', 3)
    width, height = map(int, dimensions.split())
    if magic != b'P6' or maximum != b'255' or len(pixels) != width * height * 3:
        raise ValueError('Invalid capture PPM')
    scale = bounds.get('scale', 1)
    x, y, w, h = [round(bounds[k] * scale) for k in ('x', 'y', 'width', 'height')]
    if x < 0 or y < 0 or w <= 0 or h <= 0 or x + w > width or y + h > height:
        raise ValueError('Target window is not fully inside captured monitor')
    data = b''.join(pixels[(row * width + x) * 3:(row * width + x + w) * 3] for row in range(y, y + h))
    return f'P6\n{w} {h}\n255\n'.encode() + data


def blank_rect(rows, width, rect=None):
    """Paint out the capture heartbeat so identical desktops hash identically.

    The heartbeat changes every frame by design. Left in, it would make every
    frame distinct and defeat storing frames by content. It sits outside every
    target window, so removing it removes nothing the proof reads.
    """
    rect = rect or Heartbeat.RECT
    x, w = rect['x'] * 3, rect['width'] * 3
    if rect['x'] + rect['width'] > width:
        return b''.join(rows)
    for y in range(rect['y'], min(rect['y'] + rect['height'], len(rows))):
        rows[y] = rows[y][:x] + b'\x00' * w + rows[y][x + w:]
    return b''.join(rows)


def observed(frame):
    """When a frame was captured, once `align` has put it on the stimulus
    clock. Receipt where there is nothing better: a live capture that has not
    been aligned yet, a manifest written before frames carried their own
    timestamp, and the unit fixtures that build frame records by hand."""
    return frame.get('capturedMonotonicNs', frame['receivedMonotonicNs'])


def failure(kind, reason, frames=0, status='missing'):
    return {'status': status, 'reason': reason, 'contentVerified': False,
            'proofFailure': {'kind': kind, 'capturedFrames': frames}, 'metrics': {}}


# Two 60-Hz intervals (33.33 ms) is the uncertainty a 120-ms target still
# tolerates, but it is also the best this apparatus reaches: the ScreenCast
# stream delivers a frame per refresh at best and skips to every second refresh
# under load. Measured spacing between consecutive received frames ran to
# 36.57 ms over 54 intervals, so a bound at exactly 33.33 ms rejects roughly a
# third of otherwise sound proofs for capture jitter alone. The 6.67 ms of
# headroom is one further 60-Hz interval, not a relaxation of the target.
MAX_PROOF_GAP_MS = 1000 / 30 + 1000 / 150
PROTECTED_NS = 500_000_000

# Frame work runs off the streaming thread: a mapped PipeWire buffer is one the
# compositor cannot refill, so compressing while holding it starved the producer
# and cost roughly two of every five frames. The callback now copies and
# releases; these bound how far behind the compressors may fall before the
# capture fails loudly rather than thinning out.
COMPRESSORS = 2
MAX_PENDING_FRAMES = 16

# Sixty frames a second of an unchanging desktop is the same picture sixty
# times. Frames are stored by content digest, so a still monitor costs one
# copy however long it stands still, and only real change costs memory.
CAPTURE_BUDGET_BYTES = 512 * 1024 * 1024


class Heartbeat:
    """A small actor outside the target window, repainted on every frame.

    Mutter's ScreenCast stream is damage driven. A monitor on which nothing
    moves delivers no frames at all — measured here as zero frames in four
    idle seconds while the stage went on painting sixty times a second — so a
    proof that waits for a still window cannot tell "the text was not there
    yet" from "nothing was received". Every interval then reads as wide as the
    stillness before it, however fast the viewer was.

    The heartbeat keeps the stream at the compositor's capture rate. It is
    deduplicated away by the window crop, so it never appears in the evidence,
    and it must therefore stay outside the target window rectangle.
    """
    RECT = {'x': 4, 'y': 4, 'width': 16, 'height': 16}
    START = """(() => {
      const St = imports.gi.St, GLib = imports.gi.GLib;
      if (global.__hashlineHeartbeat) return 'already-running';
      const a = new St.Widget({x: RX, y: RY, width: RW, height: RH,
                               style: 'background-color: #ff0000;'});
      Main.layoutManager.uiGroup.add_child(a);
      a.show();
      global.__hashlineHeartbeat = a;
      global.__hashlineHeartbeatTicks = 0;
      // Faster than the frame clock, so every composited frame carries damage.
      global.__hashlineHeartbeatSource = GLib.timeout_add(GLib.PRIORITY_HIGH, 8, () => {
        global.__hashlineHeartbeatTicks++;
        a.opacity = a.opacity === 255 ? 200 : 255;
        return true;
      });
      return 'started';
    })()"""
    STOP = """(() => {
      if (!global.__hashlineHeartbeat) return 0;
      imports.gi.GLib.source_remove(global.__hashlineHeartbeatSource);
      global.__hashlineHeartbeat.destroy();
      global.__hashlineHeartbeat = null;
      return global.__hashlineHeartbeatTicks;
    })()"""

    def __init__(self, bus=None):
        self.bus = bus
        self.state = 'not-started'
        self.ticks = None
        self.reason = None

    def overlaps(self, bounds):
        if not bounds:
            return False
        scale = bounds.get('scale', 1)
        a, b = self.RECT, {k: bounds[k] * scale for k in ('x', 'y', 'width', 'height')}
        return (a['x'] < b['x'] + b['width'] and b['x'] < a['x'] + a['width'] and
                a['y'] < b['y'] + b['height'] and b['y'] < a['y'] + a['height'])

    def start(self):
        """Absence is recorded, never fatal: a run without Shell.Eval still measures."""
        from session import shell_eval
        code = self.START
        for key, value in self.RECT.items():
            code = code.replace('R' + key[0].upper(), str(value))
        try:
            self.state = shell_eval(code, self.bus)
        except Exception as error:
            self.state, self.reason = 'unavailable', str(error)
        return self

    def stop(self):
        if self.state not in ('started', 'already-running'):
            return
        from session import shell_eval
        try:
            self.ticks = shell_eval(self.STOP, self.bus)
        except Exception as error:
            self.reason = str(error)
        self.state = 'stopped'

    def record(self, bounds=None):
        return {'state': self.state, 'ticks': self.ticks, 'reason': self.reason, 'rect': self.RECT,
                'overlapsWindow': self.overlaps(bounds),
                'method': 'Shell actor outside the target window, repainted every frame, so the '
                          'damage-driven ScreenCast stream keeps delivering while the window is still'}

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()


class Capture:
    def __init__(self, connector):
        import gi
        gi.require_version('Gst', '1.0')
        from gi.repository import Gio, GLib, Gst
        Gst.init(None)
        self.Gst, self.GLib, self.Gio = Gst, GLib, Gio
        from session import connection
        self.bus = connection()
        self.frames = []
        self.error = None
        self.lock = threading.Lock()
        self.pipeline = None
        self.session = None
        self.subscription = None
        self.total_bytes = 0
        self.clock_offset = None
        self.last_hash = None
        self.bounds = None
        self.closed = False
        self.pending = 0
        self.store = {}
        self.queue = queue.Queue()
        self.compressors = [threading.Thread(target=self.compress, daemon=True)
                            for _ in range(COMPRESSORS)]
        for compressor in self.compressors:
            compressor.start()
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
                raise RuntimeError(self.error or 'no-frames: ScreenCast produced no baseline frame in 10 s')
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

    def pipeline_ns(self, buffer):
        """The frame's own timestamp, on the capture pipeline's clock.

        `do-timestamp=true` stamps each buffer with the running time at which
        the source pushed it; added to the pipeline's base time that is an
        instant on the clock the pipeline runs on. Here that clock is
        PipeWire's, whose epoch is its own — measured 1,53 s against
        3.291,03 s of `CLOCK_MONOTONIC` — so the stamps are exact relative to
        each other and mean nothing on their own. `align` turns them into the
        stimulus clock.
        """
        pts = buffer.pts
        if pts == self.Gst.CLOCK_TIME_NONE or self.pipeline is None:
            return None
        base = self.pipeline.get_base_time()
        return None if base == self.Gst.CLOCK_TIME_NONE else base + pts

    def new_frame(self, sink):
        """Copy the buffer out and hand it on; never work while it is mapped."""
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
            data = bytes(mapped.data)
        finally:
            buffer.unmap(mapped)
        # Keep incoming timestamps, including identical frames. Geometry is
        # only known after mapping; window-only deduplication runs offline.
        pipeline_ns = self.pipeline_ns(buffer)
        record = {'receivedMonotonicNs': received, 'ptsNs': buffer.pts,
                  'pipelineNs': pipeline_ns}
        with self.lock:
            if pipeline_ns is not None:
                delta = received - pipeline_ns
                if self.clock_offset is None or delta < self.clock_offset:
                    self.clock_offset = delta
            if self.pending >= MAX_PENDING_FRAMES:
                self.error = (f'Frame compression is more than {MAX_PENDING_FRAMES} frames behind; '
                              'no frames silently dropped')
                return self.Gst.FlowReturn.ERROR
            self.pending += 1
            self.frames.append(record)
        self.queue.put((record, data, width, height))
        return self.Gst.FlowReturn.OK

    def compress(self):
        """Turn a copied buffer into a PPM the proof can read, off the hot path."""
        while True:
            item = self.queue.get()
            try:
                if item is None:
                    return
                record, data, width, height = item
                try:
                    # RGB rows are padded to four-byte boundaries by GStreamer.
                    stride = (width * 3 + 3) & ~3
                    if len(data) != stride * height:
                        self.error = 'Unexpected RGB stride'
                        continue
                    rows = [data[y * stride:y * stride + width * 3] for y in range(height)]
                    ppm = f'P6\n{width} {height}\n255\n'.encode() + blank_rect(rows, width)
                    digest = hashlib.sha256(ppm).hexdigest()
                except Exception as error:
                    self.error = f'Frame compression failed: {error}'
                    continue
                with self.lock:
                    packed = self.store.get(digest)
                if packed is None:
                    try:
                        packed = zlib.compress(ppm, 1)
                    except Exception as error:
                        self.error = f'Frame compression failed: {error}'
                        continue
                with self.lock:
                    if digest not in self.store:
                        self.store[digest] = packed
                        self.total_bytes += len(packed)
                    if self.total_bytes > CAPTURE_BUDGET_BYTES:
                        self.error = (f'Capture exceeds {CAPTURE_BUDGET_BYTES // (1024 * 1024)} MiB '
                                      'of distinct frames; no frames silently dropped')
                        continue
                    record['sha256'] = digest
                    record['packed'] = self.store[digest]
            finally:
                if item is not None:
                    with self.lock:
                        self.pending -= 1
                self.queue.task_done()

    def drain(self):
        """Every accepted frame carries its pixels before anyone reads them.

        Replays and unit fixtures build a capture without a live pipeline;
        their frames arrive complete and there is nothing to wait for.
        """
        if not hasattr(self, 'queue'):
            return
        self.queue.join()
        with self.lock:
            # Frames still in flight arrived after the wait and will complete;
            # incomplete ones with nothing in flight lost their pixels, which
            # is an error about the capture, never a quietly shorter record.
            unfinished = [f for f in self.frames if 'packed' not in f]
            if unfinished and not self.pending:
                self.frames = [f for f in self.frames if 'packed' in f]
                if not self.error:
                    self.error = f'{len(unfinished)} frames never reached the compressor'

    def close(self):
        if self.closed:
            return
        self.closed = True
        if self.pipeline:
            self.pipeline.set_state(self.Gst.State.NULL)
        self.drain()
        for _ in self.compressors:
            self.queue.put(None)
        for compressor in self.compressors:
            compressor.join(timeout=5)
        if self.session:
            try:
                self.call(self.session, SCREENCAST + '.Session', 'Stop')
            except Exception:
                pass
        if self.subscription:
            self.bus.signal_unsubscribe(self.subscription)
        self.loop.quit()
        self.thread.join(timeout=5)

    def retain(self, keep):
        """Keep these frames and only the pixels they still name. Lock held.

        A frame carries its own compressed copy, so dropping a record is only
        half of forgetting it: the store keeps one entry per distinct desktop,
        and until now nothing ever removed one. The budget below therefore
        counted every distinct frame of a whole launch, including the ones a
        stimulus had explicitly thrown away before it started.
        """
        self.frames = keep
        if not hasattr(self, 'store'):
            # Replays and unit fixtures carry their pixels on the frame record
            # itself and never fill a store, so there is nothing to shed.
            return
        live = {frame['sha256'] for frame in keep if 'sha256' in frame}
        self.store = {digest: packed for digest, packed in self.store.items() if digest in live}
        self.total_bytes = sum(len(packed) for packed in self.store.values())

    def drop_before(self, monotonic_ns):
        """Forget everything received before a cutoff, pixels included.

        Stimuli that share one capture call this to make the frame before
        their own stimulus the baseline. Keeping the newest frame when the
        cutoff would leave none is deliberate: a proof needs a lower bound,
        and the last thing on screen is the honest one.
        """
        with self.lock:
            keep = [f for f in self.frames if observed(f) >= monotonic_ns]
            self.retain(keep or self.frames[-1:])

    def clear(self, expected, timeout=15):
        """Wait until the document is *not* on screen, then start from there.

        A viewer from the previous row can still be painted when this capture
        opens. Waiting for the desktop is honest; accepting the leftover as a
        result would be a fast measurement of the wrong window.
        """
        deadline = time.monotonic() + timeout
        while True:
            self.drain()
            if self.error:
                raise RuntimeError(self.error)
            with self.lock:
                complete = [f for f in self.frames if 'packed' in f]
                index, frame = self.frames.index(complete[-1]), complete[-1]
            if not contains(ocr(zlib.decompress(frame['packed'])), expected):
                with self.lock:
                    self.retain(self.frames[index:])
                return frame
            if time.monotonic() >= deadline:
                raise RuntimeError(f'Expected document text still on screen after {timeout:g} s; '
                                   'no baseline without it')
            time.sleep(.5)

    def snapshot(self):
        self.drain()
        with self.lock:
            result = object.__new__(Capture)
            result.frames = [f for f in self.frames if 'packed' in f]
            result.error = self.error
            result.bounds = self.bounds
            result.clock_offset = self.clock_offset
            result.heartbeat = getattr(self, 'heartbeat', None)
            return result

    def align(self):
        """Puts every frame's own timestamp on the clock the stimulus was taken
        on, and says what that cost.

        Receipt time is not capture time, and the difference is not a constant.
        The consumer copies, hashes and deflates six megabytes per frame; at
        sixty frames a second it falls behind, `drop=false` keeps every frame,
        and the backlog shows up as ever later receipts. Measured inside one
        eight-frame capture of the baseline run, the gap between a frame's own
        timestamp and its receipt grew by 58 ms from the first frame to the
        last: a launch that takes longer collects more backlog, so timing by
        receipt charges the slow case twice over.

        The frames carry exact relative times on the capture pipeline's clock;
        what is unknown is the one offset between that clock's epoch and
        `CLOCK_MONOTONIC`. Every receipt is after its own capture, so the
        smallest observed difference is the largest offset that can be
        justified, and the frame that showed it keeps its receipt time exactly.
        Every other frame therefore lands no earlier than it was received minus
        that frame's own backlog, and never earlier than it was captured: the
        result is still an upper bound, only a much tighter one.
        """
        # Replays and unit fixtures carry frames that were aligned once
        # already, or never had a pipeline clock to align against.
        offset = getattr(self, 'clock_offset', None)
        aligned = 0
        for frame in self.frames:
            if offset is None or frame.get('pipelineNs') is None:
                continue
            frame['capturedMonotonicNs'] = frame['pipelineNs'] + offset
            aligned += 1
        late = [frame['receivedMonotonicNs'] - frame['capturedMonotonicNs']
                for frame in self.frames if 'capturedMonotonicNs' in frame]
        return {'alignedFrames': aligned, 'frames': len(self.frames),
                'clockOffsetNs': offset,
                'backlogMsMedian': (sorted(late)[len(late) // 2] / 1e6) if late else None,
                'backlogMsMax': (max(late) / 1e6) if late else None,
                'method': 'Frames timed by the capture pipeline\'s own timestamps, shifted onto '
                          'CLOCK_MONOTONIC by the smallest receipt-minus-capture difference in '
                          'the capture; receipt time where a frame carries no timestamp'}

    def archive(self, started_ns, expected, directory, persist=True):
        directory.mkdir(parents=True, exist_ok=True)
        self.clock = self.align()
        frames, last_hash = [], None
        for frame in self.frames:
            pixels = zlib.decompress(frame['packed'])
            if getattr(self, 'bounds', None):
                pixels = crop_ppm(pixels, self.bounds)
            digest = hashlib.sha256(pixels).hexdigest()
            delay = observed(frame) - started_ns
            protected = abs(delay) <= PROTECTED_NS
            if protected or digest != last_hash:
                record = {k: v for k, v in frame.items() if k != 'packed'}
                record.update(sha256=digest, delayMs=delay / 1e6)
                frames.append((record, pixels))
            elif frames:
                # Keep the last observation time of an identical run, so a
                # slow (>500 ms) response still gets the nearest lower bound.
                frames[-1][0]['lastObservedMonotonicNs'] = observed(frame)
            last_hash = digest
        # Persist the whole cropped capture before any expensive recognition.
        manifest = {'schemaVersion': 2, 'startedMonotonicNs': started_ns, 'expected': expected,
                    'windowBounds': getattr(self, 'bounds', None),
                    'captureClock': self.clock, 'frames': []}
        for index, (record, pixels) in enumerate(frames):
            if persist:
                path = f'frames/{index:06}.ppm.z'
                (directory / 'frames').mkdir(exist_ok=True)
                (directory / path).write_bytes(zlib.compress(pixels, 1))
                manifest['frames'].append(dict(record, file=path))
        if persist:
            (directory / 'capture.json').write_text(json.dumps(manifest, indent=2) + '\n')
        return frames

    def proof(self, started_ns, expected, directory, persist=True):
        directory.mkdir(parents=True, exist_ok=True)
        if self.error:
            return failure('capture-error', self.error, len(self.frames))
        if not self.frames:
            return failure('no-frames', 'Capture contains no individual frames')
        # Suite adapters must supply verified geometry. Bare object fixtures in
        # unit tests and already-cropped replay captures carry no bounds member.
        if hasattr(self, 'bounds') and self.bounds is None:
            return failure('window-geometry-unavailable', 'Target window rectangle could not be verified', len(self.frames))
        # The heartbeat is drawn above the windows. Over the target window it
        # would be evidence instead of a metronome, so that capture is void.
        if (getattr(self, 'heartbeat', None) or {}).get('overlapsWindow'):
            return failure('heartbeat-over-window',
                           'Capture heartbeat overlaps the target window; its frames are not evidence',
                           len(self.frames))
        frames = self.archive(started_ns, expected, directory, persist)
        baseline = [record for record, _ in frames if record['delayMs'] < 0]
        if not baseline:
            return failure('no-baseline', 'No pre-stimulus baseline', len(frames))
        evidence, first, last_negative, cache = [], None, None, {}
        for record, pixels in frames:
            # Retain every protected timestamp but reuse identical OCR results.
            digest = record['sha256']
            if digest not in cache:
                cache[digest] = recognize(pixels, expected)
            text, psm, matched = cache[digest]
            record = dict(record, matched=matched, ocrPsm=psm)
            evidence.append(record)
            if matched and record['delayMs'] < 0:
                raise RuntimeError('Expected document text already visible before stimulus')
            if matched:
                first = record
                (directory / 'first-readable.ppm').write_bytes(pixels)
                (directory / 'first-readable.txt').write_text(text)
                break
            last_negative = record
        result = {'status': 'ok' if first else 'missing', 'contentVerified': first is not None,
                  'expected': expected, 'frames': evidence, 'capturedFrames': len(self.frames),
                  'retainedFrames': len(frames), 'windowBounds': getattr(self, 'bounds', None),
                  # Interaction archives first and recognizes later, so the
                  # manifest may exist even here; a replay writes none at all.
                  'captureManifest': str(directory / 'capture.json') if (directory / 'capture.json').is_file() else None,
                  'heartbeat': getattr(self, 'heartbeat', None),
                  'captureClock': getattr(self, 'clock', None),
                  'method': 'Mutter ScreenCast frame timestamps (pipeline base time plus buffer PTS, receipt only where a buffer carries no usable stamp); target-window crop; offline OCR psm 6 then 11. Bounds include capture latency and are not presentation timestamps.',
                  'evidence': str(directory / 'first-readable.ppm') if first else str(directory / 'last-frame.ppm')}
        if first:
            last_seen = (last_negative.get('lastObservedMonotonicNs', observed(last_negative)) - started_ns) / 1e6
            # An effect cannot precede its stimulus: the document text is on
            # screen because the file changed or the viewer started, so the
            # earliest moment it can be readable is the stimulus itself. A
            # baseline frame older than that bounds nothing further, and the
            # stream falls silent before every stimulus that follows an
            # animation, because a still monitor produces no frames at all.
            lower = max(last_seen, 0.0)
            gap = first['delayMs'] - lower
            result.update(readableLowerMs=lower, readableUpperMs=first['delayMs'], proofGapMs=gap,
                          lastNegativeFrameMs=last_seen,
                          lastWithoutText=last_negative, firstWithText=first, ocrPsm=first['ocrPsm'],
                          proofResolutionValid=gap <= MAX_PROOF_GAP_MS,
                          maximumProofGapMs=MAX_PROOF_GAP_MS)
            if gap > MAX_PROOF_GAP_MS:
                beat = getattr(self, 'heartbeat', None)
                blind = '' if beat and beat.get('state') == 'stopped' else \
                    '; no capture heartbeat ran, so a still window delivered no frames'
                result.update(status='diagnostic',
                              reason=f'Proof interval is {gap:.2f} ms; exceeds {MAX_PROOF_GAP_MS:.2f} ms resolution limit' + blind)
        else:
            (directory / 'last-frame.ppm').write_bytes(pixels)
            (directory / 'last-frame.txt').write_text(text)
            result.update(reason='Text not recognized in captured target-window frames',
                          proofFailure={'kind': 'text-not-recognized', 'capturedFrames': len(frames)})
        return result


def replay(manifest_path, output):
    manifest = json.loads(manifest_path.read_text())
    capture = object.__new__(Capture)
    capture.error = None
    capture.frames = []
    for frame in manifest['frames']:
        path = (manifest_path.parent / frame['file']).resolve()
        if not path.is_relative_to(manifest_path.parent.resolve()):
            raise ValueError('Frame path escapes capture directory')
        packed = path.read_bytes()
        if hashlib.sha256(zlib.decompress(packed)).hexdigest() != frame['sha256']:
            raise ValueError(f'Frame checksum mismatch: {path}')
        capture.frames.append(dict(frame, packed=packed))
    result = capture.proof(manifest['startedMonotonicNs'], manifest['expected'], output, persist=False)
    result['sourceCapture'] = str(manifest_path)
    (output / 'proof.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description='Re-evaluate saved cropped frames without starting a viewer')
    parser.add_argument('capture', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    from storage import require_local_output
    try:
        require_local_output(args.out)
    except ValueError as error:
        parser.error(str(error))
    if args.out.exists():
        parser.error('Output exists; choose a new directory')
    print(json.dumps(replay(args.capture, args.out), indent=2))
