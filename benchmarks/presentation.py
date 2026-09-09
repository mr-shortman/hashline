"""Application-attributed wp_presentation feedback on CLOCK_MONOTONIC.

Feedback object IDs are joined to the requesting wl_surface, which must be an
xdg_toplevel. Cursor buffers, popups and other clients cannot become app frames.
"""
import re


def presentations(trace):
    clock = re.search(r'wp_presentation#\d+\.clock_id\((\d+)\)', trace)
    if not clock or int(clock[1]) != 1:
        raise ValueError('wp_presentation clock is not CLOCK_MONOTONIC (1)')
    xdg = {}
    main = None
    pending = {}
    frames = []
    for line in trace.splitlines():
        match = re.search(r'get_xdg_surface\(new id xdg_surface#(\d+), wl_surface#(\d+)\)', line)
        if match:
            xdg[match[1]] = match[2]
        match = re.search(r'xdg_surface#(\d+)\.get_toplevel\(', line)
        if match and main is None:
            main = xdg.get(match[1])
        match = re.search(r'wp_presentation#\d+\.feedback\(wl_surface#(\d+), new id wp_presentation_feedback#(\d+)\)', line)
        if match:
            pending[match[2]] = match[1]
        match = re.search(r'wp_presentation_feedback#(\d+)\.presented\(([^)]+)\)', line)
        if match:
            surface = pending.pop(match[1], None)
            if surface != main or main is None:
                continue
            high, low, nanos, refresh, seq_high, seq_low, flags = map(int, match[2].split(','))
            frames.append({'monotonicNs': ((high << 32) | low) * 1_000_000_000 + nanos,
                           'surface': surface, 'refreshNs': refresh,
                           'sequence': (seq_high << 32) | seq_low, 'flags': flags})
        match = re.search(r'wp_presentation_feedback#(\d+)\.discarded\(', line)
        if match:
            pending.pop(match[1], None)
    return sorted({frame['monotonicNs']: frame for frame in frames}.values(), key=lambda frame: frame['monotonicNs'])


def analyze(trace, samples, hz):
    frames = presentations(trace)
    times = [frame['monotonicNs'] for frame in frames]
    budget = 16.7 if hz < 100 else 8.37
    rows = []
    for sample in samples:
        start = sample['commandStartMonotonicNs'] + 1_000_000_000
        end = start + 10_000_000_000
        if end >= sample['observedEndMonotonicNs']:
            raise ValueError('Scroll window has no full ten-second interior')
        before = [stamp for stamp in times if stamp <= start]
        after = [stamp for stamp in times if stamp >= end]
        if not before or not after:
            raise ValueError('Application presentations do not cover the complete scroll window')
        stamps = [before[-1], *[stamp for stamp in times if start < stamp < end], after[0]]
        intervals = [(right - left) / 1e6 for left, right in zip(stamps, stamps[1:])]
        # Missing refresh slots count as missed frames, not a single bad interval.
        # A one-second stall must not pass among 599 otherwise timely frames.
        slots = [max(1, round(interval / (1000 / hz))) for interval in intervals]
        within = sum(interval <= budget for interval in intervals)
        rows.append({'startMonotonicNs': start, 'endMonotonicNs': end,
                     'intervalsMs': intervals, 'refreshSlots': sum(slots),
                     'withinBudgetPercent': 100 * within / sum(slots),
                     'maximumGapMs': max(intervals)})
    return {'applicationFramesVerified': True, 'clock': 'CLOCK_MONOTONIC',
            'method': 'wp_presentation feedback joined to first xdg_toplevel surface; ten-second interior; missed refresh slots included in denominator',
            'budgetMs': budget, 'frames': frames, 'raw': rows}
