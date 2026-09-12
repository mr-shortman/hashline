"""Shared measurement adapters. bench/compare differ only in matrix selection."""
import contextlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

from content import PROTECTED_NS, Capture, Heartbeat, failure
from memory import ISOLATION, PrivateBus, measure as memory_measure

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / (name + '.py'))
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


startup = module('startup-wayland')

from content import EXPECTED



@contextlib.contextmanager
def isolated(renderer, connector):
    old = dict(os.environ)
    bus = PrivateBus()
    with tempfile.TemporaryDirectory(prefix='hashline-bench-') as home:
        try:
            os.environ['HASHLINE_COMPOSITOR_BUS_ADDRESS'] = old.get('DBUS_SESSION_BUS_ADDRESS', '')
            os.environ.update(ISOLATION)
            os.environ.update(DBUS_SESSION_BUS_ADDRESS=bus.address, GDK_BACKEND='wayland',
                              QT_QPA_PLATFORM='wayland', WINIT_UNIX_BACKEND='wayland', HASHLINE_MONITOR=connector)
            for name in ('CONFIG', 'DATA', 'STATE', 'CACHE'):
                path = Path(home) / name.lower(); path.mkdir()
                os.environ[f'XDG_{name}_HOME'] = str(path)
            if renderer in ('default', None):
                os.environ.pop('GSK_RENDERER', None)
            else:
                os.environ['GSK_RENDERER'] = renderer
            yield bus, home
        finally:
            os.environ.clear(); os.environ.update(old)
            bus.stop()


def stop(process):
    if process is None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()


def external(command, path, timeout=180, env=None):
    with path.open('w') as log:
        process = subprocess.Popen(command, cwd=REPO, env=env, stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except BaseException:
            stop(process)
            raise
    if code:
        raise RuntimeError(f'{command[0]} exited {code}; see {path}')


def readable(command, fixture, renderer, options, artifact, action=None, proof=True):
    """One launch, two readings: protocol marks always, screen content on request.

    Without `proof` this is `startup-wayland.py`: when the compositor scanned a
    frame out, whatever it contained. With it, the frame must show the document.
    """
    if proof:
        # Capture uses the real session bus; the tested process uses a private bus.
        pointer = module('scroll-native').Pointer(options.connector)
        try:
            width, height = module('scroll-native').monitor_geometry(options.connector)
            pointer.to(width / 2, height / 2)
        finally:
            pointer.close()
    capture = Capture(options.connector) if proof else None
    # Started before the baseline: without it the stream falls silent whenever
    # the window stops moving, and every interval spanning that silence is
    # wider than the resolution limit no matter how fast the viewer answered.
    beat = Heartbeat(capture.bus).start() if capture else None
    application = None
    trace = artifact / 'wayland.log'
    try:
        if capture and action is None:
            # The launch is the stimulus, so the screen must be free of the
            # document first — the previous row's window may still be painted.
            # The heartbeat brings the stream to its full cadence within about
            # 200 ms, so half a second is again enough; it was raised to three
            # only to outwait a ramp that the starved consumer was causing.
            time.sleep(.5)  # Keep a real pre-launch baseline; recognize only after termination.
        with isolated(renderer, options.connector) as (bus, home), trace.open('w+') as sink:
            env = dict(os.environ, WAYLAND_DEBUG='1', HASHLINE_BENCH_METADATA='1')
            if proof:
                env.pop('NO_AT_BRIDGE', None)
                env['GTK_A11Y'] = 'atspi'
            started_ns = time.monotonic_ns()
            started_ms = startup.realtime_ms()
            application = subprocess.Popen([*command, str(fixture)], env=env,
                stdout=subprocess.DEVNULL, stderr=sink, start_new_session=True)
            expected = EXPECTED.get(fixture.stem, ['Benchmark document text'])
            if action:
                time.sleep(options.settle)
                if application.poll() is not None:
                    return failure('program-exited', 'Viewer exited before stimulus', len(capture.frames))
                started_ns, expected = action(application, capture, fixture)
            time.sleep(options.hold)
            exited = application.poll()
            if capture and exited is None:
                from session import window_bounds
                capture.bounds = window_bounds(application.pid, capture.bus)
            stop(application); application = None
            sink.seek(0)
            trace_text = sink.read()
            marks = startup.parse(trace_text, started_ms, 150)
            metadata = re.search(r'HASHLINE_BENCH renderer=(\S+) backend=(\S+)', trace_text)
        if capture:
            beat.stop()
            capture.heartbeat = beat.record(capture.bounds)
            capture.close()
            if exited is not None:
                result = failure('program-exited', f'Viewer exited before termination: {exited}', len(capture.frames))
            elif marks.get('frame') is None and marks.get('presented') is None:
                result = failure('window-never-mapped', 'No application frame callback or presentation during observation', len(capture.frames), 'unrenderable')
                result['observationSeconds'] = options.hold
            else:
                result = capture.proof(started_ns, expected, artifact)
            result['metrics'] = {key: result[key] for key in ('readableLowerMs', 'readableUpperMs', 'proofGapMs')} if result['contentVerified'] else {}
        elif marks.get('presented') is None:
            result = {'status': 'missing', 'reason': 'No wp_presentation_feedback.presented in the trace', 'metrics': {}}
        else:
            result = {'status': 'ok', 'contentVerified': False, 'metrics': {'presentedMs': marks['presented']},
                      'method': 'Wayland protocol marks only; nothing proves the frame showed the document'}
        result['protocol'] = marks
        result['exitBeforeTermination'] = exited
        if exited is not None:
            result.update(failure('program-exited', f'Viewer exited before termination: {exited}', len(capture.frames) if capture else 0))
        result['backendObserved'] = metadata[2] if metadata else ('Wayland' if marks else 'unknown')
        result['rendererObserved'] = metadata[1] if metadata else 'unknown'
        return result
    finally:
        stop(application)
        if beat:
            beat.stop()
        if capture:
            capture.close()


def keyboard(pointer, keys):
    try:
        for key in keys:
            pointer.call('NotifyKeyboardKeysym', '(ub)', (key, True))
    finally:
        for key in reversed(keys):
            pointer.call('NotifyKeyboardKeysym', '(ub)', (key, False))


def require_focus(pid):
    # The private compositor can identify the focused surface directly, even
    # for clients that do not publish accessibility state.
    if os.environ.get('HASHLINE_SESSION_KIND') == 'nested-headless':
        from session import connection, shell_eval
        bus = connection(os.environ['HASHLINE_COMPOSITOR_BUS_ADDRESS'])
        if shell_eval(f'global.display.focus_window?.get_pid() === {int(pid)}', bus):
            return
        raise RuntimeError('Nested compositor focus does not belong to benchmark process; no keys sent')
    # A fresh helper avoids caching AT-SPI addresses across private buses.
    code = """
import gi, sys, time
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

def walk(node):
    yield node
    for i in range(node.get_child_count()):
        yield from walk(node.get_child_at_index(i))
for attempt in range(20):
    for node in walk(Atspi.get_desktop(0)):
        if node.get_process_id() == int(sys.argv[1]) and node.get_state_set().contains(Atspi.StateType.ACTIVE):
            sys.exit(0)
    time.sleep(.1)
sys.exit(1)
"""
    env = dict(os.environ)
    env.pop('NO_AT_BRIDGE', None)
    result = subprocess.run(['/usr/bin/python3', '-c', code, str(pid)], env=env,
                            capture_output=True, timeout=8)
    if result.returncode:
        raise RuntimeError('Cannot verify benchmark window focus via AT-SPI; no keys sent')


def interaction(command, fixture, renderer, options, artifact):
    """Four stimuli in one launch, each timed to the frame that shows its effect.

    They share a launch because each needs a settled window and a private bus,
    and four launches would be four times the waiting for numbers that are all
    about the same running program: opening the search bar, opening the menu,
    a search that has to jump, and handing a second file to the instance that
    is already running.

    The longest main-thread task is the fifth reading, and the only one that
    cannot come from outside: a compositor sees late frames, not what made them
    late. The reader reports its own under HASHLINE_BENCH_MAIN_THREAD.
    """
    # A needle that occurs once, at the end, so the search must actually move
    # the document to satisfy it. Its prefixes all match that same line, which
    # is why the decoy exists: the reader types at 50 ms and the viewer bundles
    # queries for 60, so a keystroke gap that slips past the bundling window
    # runs a search on a prefix. Without somewhere else for such a search to
    # land it would show the target before the stimulus that is supposed to
    # reveal it, and the proof would rightly refuse to time it. That cost one
    # cell of 93. The decoy carries every prefix and not the needle, and sits
    # at the top where the reading position already is, so an early search
    # moves nothing and the last keystroke is still what makes the jump.
    needle = 'Kaninchenbau'
    decoy = 'Kaninchenbahn'
    assert decoy.startswith(needle[:-1]) and needle not in decoy
    pointer = module('scroll-native').Pointer(options.connector)
    application = None
    capture = Capture(options.connector)
    beat = Heartbeat(capture.bus).start()
    readings, evidence, pending = {}, {}, []
    trace = artifact / 'wayland.log'
    try:
        width, height = module('scroll-native').monitor_geometry(options.connector)
        pointer.to(width / 2, height / 2)
        with tempfile.TemporaryDirectory(prefix='hashline-interaction-') as temp, \
                isolated(renderer, options.connector) as (bus, home), trace.open('w+') as sink:
            document = Path(temp) / fixture.name
            document.write_text(f'Ablenkung {decoy} oben.\n\n' + fixture.read_text()
                                + f'\n\nSuchziel {needle} Ende.\n')
            second = Path(temp) / ('zweites-' + fixture.name)
            second.write_text('Zweites Dokument beginnt hier.\n\n' + fixture.read_text())
            for image in fixture.parent.glob('*.png'):
                (Path(temp) / image.name).symlink_to(image)
            env = dict(os.environ, HASHLINE_BENCH_METADATA='1', HASHLINE_BENCH_MAIN_THREAD='1')
            env.pop('NO_AT_BRIDGE', None)
            env['GTK_A11Y'] = 'atspi'
            application = subprocess.Popen([*command, str(document)], env=env,
                stdout=subprocess.DEVNULL, stderr=sink, start_new_session=True)
            time.sleep(options.settle)
            if application.poll() is not None:
                raise RuntimeError('Viewer exited before the first stimulus')
            require_focus(application.pid)
            from session import window_bounds
            capture.bounds = window_bounds(application.pid, capture.bus)

            def stimulus(name, expected, act):
                capture.drop_before(time.monotonic_ns() - PROTECTED_NS)
                started = act()
                time.sleep(options.hold)
                pending.append((name, started, expected, capture.snapshot()))

            def open_search():
                started = time.monotonic_ns()
                keyboard(pointer, [65507, ord('f')])
                return started
            # Not the leading "Im": its capital I stands right against the
            # magnifier in the entry, and OCR reads the pair as "}m", "[m" or
            # "(.Q |m". While the bar still slid into place that cost nothing,
            # because the clipped bar was unreadable anyway and both phrases
            # first matched on the very same frame. On a bar that arrives whole
            # it cost 250 ms of invented latency: the placeholder was legible
            # at 19 ms and the phrase did not match until 285.
            stimulus('searchOpenUpperMs', ['Dokument suchen'], open_search)

            def run_search():
                for char in needle[:-1]:
                    keyboard(pointer, [ord(char)])
                    time.sleep(.05)
                capture.drop_before(time.monotonic_ns() - PROTECTED_NS)
                started = time.monotonic_ns()
                keyboard(pointer, [ord(needle[-1])])
                return started
            # The words that flank the needle, never the needle itself: the
            # hit is painted under the search wash, and OCR reads the marked
            # word wrong often enough to lose a sound proof. On a frame that
            # plainly showed the line, both segmentation modes returned
            # "Suchziel Kaninehenbau Ende." and the phrase with the needle in
            # it did not match. Neither flanking word occurs in any fixture,
            # so the pair still proves only one thing: the view moved to the
            # target line.
            stimulus('searchLargeMs', ['Suchziel', 'Ende.'], run_search)
            keyboard(pointer, [65307]); time.sleep(.4)

            def open_menu():
                started = time.monotonic_ns()
                keyboard(pointer, [65479])  # F10 opens the primary menu.
                return started
            stimulus('menuOpenMs', ['Neu laden'], open_menu)
            keyboard(pointer, [65307]); time.sleep(.4)

            def hand_over():
                started = time.monotonic_ns()
                subprocess.run([*command, str(second)], env=env, timeout=60,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                return started
            stimulus('openExistingMs', ['Zweites Dokument beginnt hier'], hand_over)

            exited = application.poll()
            stop(application); application = None
            sink.seek(0)
            log = sink.read()
        beat.stop()
        capture.heartbeat = beat.record(capture.bounds)
        for _, _, _, snapshot in pending:
            snapshot.heartbeat = capture.heartbeat
        capture.close()
        for name, started, expected, snapshot in pending:
            if snapshot.bounds is not None and not snapshot.error:
                snapshot.archive(started, expected, artifact / name)
        for name, started, expected, snapshot in pending:
            proof = snapshot.proof(started, expected, artifact / name, persist=False)
            evidence[name] = proof
            readings[name] = proof.get('readableUpperMs')
        longest = re.findall(r'HASHLINE_BENCH mainThreadMaxMs=([\d.]+) task=(\S+)', log)
        metadata = re.search(r'HASHLINE_BENCH renderer=(\S+) backend=(\S+)', log)
        metrics = {name: value for name, value in readings.items() if value is not None}
        if longest:
            metrics['mainThreadMaxMs'] = float(longest[-1][0])
        missing = [name for name, value in readings.items() if value is None]
        if not longest:
            missing.append('mainThreadMaxMs')
        return {'status': 'ok' if not missing and exited is None else 'missing',
                'reason': ('No frame showed: ' + ', '.join(missing)) if missing else
                          (f'Viewer exited before termination: {exited}' if exited is not None else None),
                'proofFailure': next((p['proofFailure'] for p in evidence.values() if p.get('proofFailure')), None),
                'proofResolutionValid': all(p.get('proofResolutionValid', False) for p in evidence.values()),
                'heartbeat': capture.heartbeat,
                'longestTask': longest[-1][1] if longest else None,
                'mainThreadTasks': [{'ms': float(ms), 'task': task} for ms, task in longest],
                'evidence': evidence,
                'rendererObserved': metadata[1] if metadata else 'unknown',
                'backendObserved': metadata[2] if metadata else 'unknown',
                'metrics': metrics}
    finally:
        stop(application)
        beat.stop()
        capture.close()
        pointer.close()


def tab_memory(command, fixture, renderer, options, artifact):
    """PSS with one, two and ten copies of a fixture open at once.

    The copies are real files with their own names, because a viewer that
    already shows a path may bring that tab forward rather than open a
    second one — which would measure nothing.
    """
    import shlex
    copies = artifact / 'copies'
    copies.mkdir(parents=True, exist_ok=True)
    extras = []
    for index in range(9):
        path = copies / f'{fixture.stem}-{index}{fixture.suffix}'
        path.write_bytes(fixture.read_bytes())
        extras.append(str(path))
    readings = {}
    for tabs in (1, 2, 10):
        wrapper = artifact / f'viewer-{tabs}'
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.join(command)
                           + (' ' + shlex.join(extras[:tabs - 1]) if tabs > 1 else '') + ' "$@"\n')
        wrapper.chmod(0o755)
        with isolated(renderer, options.connector) as (bus, home):
            result = memory_measure(str(wrapper), fixture, bus,
                                    renderer if renderer != 'default' else None,
                                    options.settle, options.sample_seconds, home)
        if not (result.get('alive') and result.get('isolated') and result.get('pssKibMedian')):
            return {'status': 'missing', 'reason': f'no settled reading with {tabs} tabs',
                    'raw': result}
        readings[tabs] = result['pssKibMedian'] / 1024
    return {'status': 'ok', 'pssMiB': readings,
            'inactiveTabMiB': readings[2] - readings[1], 'tenTabsMiB': readings[10]}


def measure(group, command, fixture, renderer, spec, options, artifact, hz=None):
    artifact.mkdir(parents=True, exist_ok=True)
    if hz is not None:
        from display import verify_rate
        verify_rate(hz, options.connector)
    if group == 'mainthread':
        return mainthread(command, fixture, renderer, options, artifact)
    if group in ('startup', 'content'):
        return readable(command, fixture, renderer, options, artifact, proof=group == 'content')
    if group == 'stages':
        log = artifact / 'stages.txt'
        external([str(REPO / 'target/release/examples/measure'), '--repetitions', '1', str(fixture)], log)
        line = next((line for line in log.read_text().splitlines() if line.startswith(fixture.name)), '')
        values = re.findall(r'([\d.]+)/\s*([\d.]+)', line)
        if len(values) != 3:
            raise RuntimeError('Unrecognized measure output')
        return {'status': 'ok', 'metrics': dict(zip(('parseMs', 'planMs', 'screenMs'),
                                                   [float(pair[0]) for pair in values]))}
    if group in ('memory', 'idle'):
        # Two questions, two windows: how much a document costs to hold, and what
        # the program does while nobody touches it. Only the second needs 30 s,
        # so repeating it thirty times would be thirty times the same waiting.
        seconds = options.idle_seconds if group == 'idle' else options.sample_seconds
        # A launcher preserves argv while using the existing single-binary API.
        wrapper = artifact / 'viewer'
        import shlex
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.join(command) + ' "$@"\n'); wrapper.chmod(0o755)
        with isolated(renderer, options.connector) as (bus, home):
            result = memory_measure(str(wrapper), fixture, bus,
                                    renderer if renderer != 'default' else None,
                                    options.settle, seconds, home)
        pss = result.get('pssKibMedian')
        cpu = result.get('cpuCoreShare')
        value = ({'idleCpuPercent': cpu * 100 if cpu is not None else None} if group == 'idle'
                 else {'pssMiB': pss / 1024 if pss is not None else None})
        measured = cpu is not None if group == 'idle' else pss is not None
        return {'status': 'ok' if result.get('alive') and result.get('isolated') and measured else 'missing',
                'reason': (result.get('error') or ('Reader exited during observation' if not result.get('alive') else
                           'Isolation failed' if not result.get('isolated') else 'Process samples are unreadable or incomplete'))
                          if not (result.get('alive') and result.get('isolated') and measured) else None,
                'raw': result, 'observationSeconds': seconds,
                'rendererObserved': result.get('rendererObserved', 'unknown'),
                'backendObserved': result.get('backendObserved', 'unknown'), 'metrics': value}
    if group == 'reload':
        if not spec.get('reload'):
            return {'status': 'unsupported', 'reason': 'Live reload not supported by configured viewer', 'metrics': {}}
        # The changed text is put where the reader is, not at the top of the
        # file, and a paragraph is added above everything so that every block
        # index below it moves. A frame that shows the change therefore proves
        # two things at once: how long the reload took, and that the reading
        # position survived it (docs/metrics.md). A viewer that jumps
        # to the top after a reload shows the first section instead and fails.
        anchor = '\n\n## Ankerabschnitt\n\nAnkerzeile eins zwei drei.\n'
        changed = '\n\n## Ankerabschnitt\n\nAnkerzeile geaendert vier.\n'
        with tempfile.TemporaryDirectory(prefix='hashline-reload-') as temp:
            path = Path(temp) / fixture.name
            path.write_text(fixture.read_text() + anchor)
            for image in fixture.parent.glob('*.png'):
                (Path(temp) / image.name).symlink_to(image)
            pointer = module('scroll-native').Pointer(options.connector)
            try:
                def reload_action(application, capture, document):
                    require_focus(application.pid)
                    # Find the anchor section with the viewer's own search, so
                    # the reader is far from the top when the file changes.
                    keyboard(pointer, [65507, ord('f')]); time.sleep(.5)
                    for char in 'Ankerzeile':
                        keyboard(pointer, [ord(char)])
                    time.sleep(.6)
                    keyboard(pointer, [65293]); time.sleep(.4)
                    keyboard(pointer, [65307]); time.sleep(.4)
                    # Exclude the setup frames, keeping one pre-stimulus baseline.
                    capture.drop_before(time.monotonic_ns() - PROTECTED_NS)
                    replacement = document.with_suffix('.new')
                    replacement.write_text('Neuer Absatz oben.\n\n'
                                           + fixture.read_text() + changed)
                    started = time.monotonic_ns()
                    replacement.replace(document)
                    return started, ['Ankerzeile geaendert vier']
                result = readable(command, path, renderer, options, artifact, reload_action)
            finally:
                pointer.close()
            preserved = bool(result.get('contentVerified'))
            result['metrics'] = {'reloadUpperMs': result.get('readableUpperMs'),
                                 'reloadAnchorPreserved': preserved}
            result['anchorVerified'] = preserved
            result['stimulus'] = ('Atomic rename of a temporary copy: a paragraph added above '
                                  'everything and the paragraph at the reading position changed')
            return result
    if group == 'interaction':
        return interaction(command, fixture, renderer, options, artifact)
    if group == 'tabs':
        if not spec.get('tabs'):
            return {'status': 'unsupported', 'reason': 'Viewer has no tabs', 'metrics': {}}
        # Accessibility registry must live on the same isolated bus as the app;
        # real keyboard events still use the desktop Mutter connection.
        pointer = module('scroll-native').Pointer(options.connector)
        try:
            def action(application, capture, document):
                require_focus(application.pid)
                other = artifact / 'second-tab.md'
                other.write_text('# Second tab\n\nDifferent benchmark document body.\n')
                if spec.get('role') == 'self':
                    # Hashline's public file handoff avoids a separately owned
                    # portal dialog while exercising the actual tab-opening path.
                    subprocess.run([*command, str(other)], env=dict(os.environ), timeout=30,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
                    time.sleep(options.settle)
                else:
                    keyboard(pointer, [65507, ord('o')]); time.sleep(.5)
                    keyboard(pointer, [65507, ord('l')]); time.sleep(.2)
                    for char in str(other):
                        keyboard(pointer, [ord(char)])
                    keyboard(pointer, [65293]); time.sleep(options.settle)
                require_focus(application.pid)
                # Exclude setup frames, retaining an actual pre-switch baseline.
                capture.drop_before(time.monotonic_ns() - PROTECTED_NS)
                started = time.monotonic_ns()
                keyboard(pointer, spec.get('tab_keys', [65507, 65289]))
                return started, EXPECTED[document.stem]
            result = readable(command, fixture, renderer, options, artifact, action)
            result['tabSetup'] = 'application-command' if spec.get('role') == 'self' else 'keyboard-file-chooser'
            result['metrics'] = {'tabSwitchUpperMs': result.get('readableUpperMs')}
            if result.get('contentVerified'):
                # What a tab that is not showing costs, and what ten of them
                # cost together (docs/metrics.md). Every tab needs
                # its own path: a viewer may bring an open file forward
                # instead of opening it twice.
                held = tab_memory(command, fixture, renderer, options, artifact)
                result['tabMemory'] = held
                result['metrics'].update(
                    {'inactiveTabMiB': held.get('inactiveTabMiB'),
                     'tenTabsMiB': held.get('tenTabsMiB')})
                if held.get('status') != 'ok':
                    result['status'] = 'missing'
                    result['reason'] = held.get('reason', 'tab memory unmeasured')
            return result
        finally:
            pointer.close()
    if group in ('scroll', 'stability'):
        wrapper = artifact / 'viewer'
        import shlex
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.join(command) + ' "$@"\n'); wrapper.chmod(0o755)
        env = dict(os.environ, GDK_BACKEND='wayland', QT_QPA_PLATFORM='wayland',
                   WINIT_UNIX_BACKEND='wayland', GSETTINGS_BACKEND='memory')
        if renderer != 'default':
            env['GSK_RENDERER'] = renderer
        else:
            env.pop('GSK_RENDERER', None)
        if group == 'scroll':
            prefix = artifact / 'scroll'
            external(['/usr/bin/python3', str(ROOT / 'compositor.py'), str(wrapper), str(fixture),
                      '--connector', options.connector, *(['--refresh-hz', str(hz)] if hz is not None else []),
                      '--iterations', '1', '--content-proof', '--output-prefix', str(prefix)], artifact / 'driver.log', env=env)
            dump = artifact / 'scroll.dump'
            external(['sysprof-cat', '--no-callgraph', '--no-counters', str(prefix) + '.syscap'], dump)
            output = artifact / 'frametimes.json'
            external(['/usr/bin/python3', str(ROOT / 'analyze-compositor.py'), str(dump),
                      '--scroll', str(prefix) + '-scroll.json', '--display', str(prefix) + '-display.json',
                      '--capture', str(prefix) + '.syscap', '--output', str(output)], artifact / 'analysis.log')
            raw = json.loads(output.read_text())
            scroll = json.loads(Path(str(prefix) + '-scroll.json').read_text())
            app = scroll.get('applicationPresentation', {})
            if app.get('applicationFramesVerified') and app.get('contentVerified') and app.get('raw'):
                return {'status': 'ok', 'applicationFramesVerified': True, 'refreshHzObserved': scroll['refreshHzDeclared'],
                        'rendererObserved': scroll.get('rendererObserved', 'unknown'),
                        'backendObserved': scroll.get('backendObserved', 'unknown'), 'raw': app, 'compositor': str(output),
                        'metrics': {'scrollWithinPercent': min(r['withinBudgetPercent'] for r in app['raw']),
                                    'scrollMaxGapMs': max(r['maximumGapMs'] for r in app['raw'])}}
            return {'status': 'diagnostic', 'refreshHzObserved': scroll.get('refreshHzDeclared'), 'reason': 'Monitor-wide frames; application frame attribution remains unverified',
                    'raw': raw, 'metrics': {
                        'scrollWithinPercent': min(r['definitelyWithinBudgetPercent'] for r in raw['raw']),
                        'scrollMaxGapMs': max(r['maximumGapUpperBoundMs'] for r in raw['raw'])}}
        with isolated(renderer, options.connector):
            output = artifact / 'stability.json'
            external(['/usr/bin/python3', str(ROOT / 'stability.py'), str(wrapper), '--first', str(fixture),
                      '--output', str(output)], artifact / 'driver.log', env=dict(os.environ))
        raw = json.loads(output.read_text())
        return {'status': 'ok' if raw.get('completed') else 'missing',
                'reason': None if raw.get('completed') else raw.get('error', 'Stability switches or final samples incomplete'), 'raw': raw,
                'metrics': {'stabilityGrowthPercent': raw.get('growthPercent')}}
    raise ValueError(group)


def mainthread(command, fixture, renderer, options, artifact):
    """Read self-reported main-thread tasks for startup and real file handoff.

    No monitor pixels or input injection are needed. The scope is recorded;
    this cannot certify unobserved keyboard/menu tasks.
    """
    application = None
    with isolated(renderer, options.connector), (artifact / 'mainthread.log').open('w+') as log:
        try:
            env = dict(os.environ, HASHLINE_BENCH_MAIN_THREAD='1', HASHLINE_BENCH_METADATA='1')
            application = subprocess.Popen([*command, str(fixture)], env=env, stdout=subprocess.DEVNULL,
                                           stderr=log, start_new_session=True)
            time.sleep(options.settle)
            if application.poll() is not None:
                return {'status': 'missing', 'reason': 'Reader exited before main-thread observation', 'metrics': {}}
            subprocess.run([*command, str(fixture)], env=env, stdout=subprocess.DEVNULL, stderr=log, timeout=30)
            time.sleep(options.hold)
            exited = application.poll()
        finally:
            stop(application)
        log.seek(0)
        text = log.read()
    tasks = re.findall(r'HASHLINE_BENCH mainThreadMaxMs=([\d.]+) task=(\S+)', text)
    metadata = re.search(r'HASHLINE_BENCH renderer=(\S+) backend=(\S+)', text)
    return {'status': 'ok' if tasks and exited is None else 'missing',
            'reason': None if tasks and exited is None else 'Reader exited or emitted no main-thread task measurements',
            'scope': 'Startup and file handoff; keyboard and menu tasks are recorded separately by interaction',
            'rendererObserved': metadata[1] if metadata else 'unknown',
            'backendObserved': metadata[2] if metadata else 'unknown',
            'mainThreadTasks': [{'ms': float(ms), 'task': task} for ms, task in tasks],
            'metrics': {'mainThreadMaxMs': max(float(ms) for ms, _ in tasks)} if tasks else {}}
