#!/usr/bin/env python3
"""Startup from `exec` to pixels, over the Wayland protocol.

`examples/measure` times Hashline's own stages without a window, so it cannot
answer the question decision 009 was argued on: how long from starting the
process until the reader sees text. This driver answers it from outside the
application, with no instrumentation in the program under test, so it measures
any Wayland client the same way — Hashline and a reference viewer alike.

Method. `WAYLAND_DEBUG=1` makes libwayland print every protocol message with a
CLOCK_REALTIME stamp in milliseconds, truncated to 32 bits of microseconds. The
driver takes the same clock immediately before `exec`, so every stamp converts
to a delay since the process started. Four moments are extracted:

  attach     first `wl_surface.attach` — the first buffer handed to the compositor
  frame      first `wl_callback.done` for a callback created by `wl_surface.frame`
  presented  first `wp_presentation_feedback.presented` — pixels actually shown
  quiet      last message before the protocol first falls silent for `--quiet-ms`

`presented` is the honest one: it is the compositor reporting a scanned-out
frame, not the client guessing. The earlier reports in this repository all note
that they lack exactly this evidence.

Limits. The stamp is taken when libwayland formats the message, so the dynamic
loader and everything before the first protocol traffic are inside `attach` but
invisible on their own. A run needs its own D-Bus so single-instance handoff
cannot make a second process exit instead of drawing; pass `--bus`. Nothing here
proves the frame contained the document rather than an empty window — pair it
with the acceptance screenshots for that.
"""

import argparse
import json
import os
import re
import signal
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

# `[ 146814.210] {Default Queue}  -> wl_surface#49.attach(wl_buffer#78, 0, 0)`
LINE = re.compile(r"^\[\s*(\d+)\.(\d{3})\]\s*(?:\{[^}]*\}\s*)?(->)?\s*(\S+?)#(\d+)\.(\w+)\((.*)$")
NEW_CALLBACK = re.compile(r"new id wl_callback#(\d+)")

# libwayland prints `(unsigned int)(seconds * 1e6 + nanoseconds / 1000)`, so the
# stamp wraps every 2**32 microseconds. Runs are seconds long; one wrap is enough.
WRAP_MS = 2**32 / 1000.0


def realtime_ms() -> float:
    """The clock libwayland stamps with, in the units it prints."""
    micros = int(time.clock_gettime(time.CLOCK_REALTIME) * 1_000_000)
    return (micros % 2**32) / 1000.0


def parse(trace: str, started_ms: float, quiet_ms: float) -> dict:
    """Delays since `exec`, in milliseconds, for the four moments."""
    frame_callbacks: set[str] = set()
    marks: dict[str, float] = {}
    stamps: list[float] = []

    for line in trace.splitlines():
        found = LINE.match(line)
        if not found:
            continue
        millis, micros, outgoing, interface, ident, member, rest = found.groups()
        delay = (float(f"{millis}.{micros}") - started_ms) % WRAP_MS
        stamps.append(delay)

        if outgoing and interface == "wl_surface":
            if member == "attach":
                marks.setdefault("attach", delay)
            elif member == "frame":
                new = NEW_CALLBACK.search(rest)
                if new:
                    frame_callbacks.add(new.group(1))
        elif not outgoing:
            if interface == "wl_callback" and member == "done" and ident in frame_callbacks:
                marks.setdefault("frame", delay)
            elif interface == "wp_presentation_feedback" and member == "presented":
                marks.setdefault("presented", delay)

    # Quiet: the last message before the protocol first pauses for `quiet_ms`,
    # counted only after there is something on screen to be quiet about.
    after = marks.get("presented", marks.get("frame", 0.0))
    for previous, following in zip(stamps, stamps[1:]):
        if previous >= after and following - previous >= quiet_ms:
            marks["quiet"] = previous
            break
    else:
        if stamps:
            marks["quiet"] = stamps[-1]
    return marks


def run(command: list[str], bus: str, hold: float, quiet_ms: float) -> dict:
    environment = dict(os.environ, WAYLAND_DEBUG="1")
    if bus:
        environment["DBUS_SESSION_BUS_ADDRESS"] = bus
    # A pipe would deadlock: the trace outgrows the 64 KiB pipe buffer long
    # before the first frame, and the client blocks writing it instead of
    # drawing. That silently moves the very moment being measured.
    with tempfile.TemporaryFile("w+", errors="replace") as sink:
        started = realtime_ms()
        with subprocess.Popen(
            command,
            stdout=subprocess.DEVNULL,
            stderr=sink,
            env=environment,
        ) as process:
            time.sleep(hold)
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        sink.seek(0)
        trace = sink.read()
    return parse(trace, started, quiet_ms)


def quantiles(values: list[float]) -> dict:
    ordered = sorted(values)
    rank = max(0, min(len(ordered) - 1, -(-len(ordered) * 95 // 100) - 1))
    return {
        "n": len(ordered),
        "median": round(statistics.median(ordered), 1),
        "p95": round(ordered[rank], 1),
        "min": round(ordered[0], 1),
        "max": round(ordered[-1], 1),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("files", nargs="+")
    parser.add_argument("--repetitions", type=int, default=30)
    parser.add_argument("--hold", type=float, default=3.0, help="Seconds before SIGTERM")
    parser.add_argument("--quiet-ms", type=float, default=150.0)
    parser.add_argument("--bus", default="", help="DBUS_SESSION_BUS_ADDRESS for an isolated run")
    parser.add_argument("--settle", type=float, default=0.6, help="Seconds between runs")
    parser.add_argument("--label", default="")
    parser.add_argument("--output", type=Path)
    options = parser.parse_args()
    from storage import require_local_output
    if options.output:
        try:
            require_local_output(options.output)
        except ValueError as error:
            parser.error(str(error))

    moments = ("attach", "frame", "presented", "quiet")
    result = {
        "binary": options.binary,
        "label": options.label or Path(options.binary).name,
        "method": (
            "WAYLAND_DEBUG protocol stamps against CLOCK_REALTIME at exec. "
            "'presented' is wp_presentation_feedback, i.e. the compositor "
            "reporting a shown frame. No proof of what the frame contained."
        ),
        "repetitions": options.repetitions,
        "holdSeconds": options.hold,
        "quietMs": options.quiet_ms,
        "isolatedBus": bool(options.bus),
        "fixtures": {},
    }

    for path in options.files:
        samples: dict[str, list[float]] = {name: [] for name in moments}
        missing = 0
        # `none` starts the viewer with no document, which separates the cost of
        # the toolkit and the window from the cost of the document itself.
        command = [options.binary] if path == "none" else [options.binary, path]
        for _ in range(options.repetitions):
            marks = run(command, options.bus, options.hold, options.quiet_ms)
            if "presented" not in marks:
                missing += 1
            for name in moments:
                if name in marks:
                    samples[name].append(marks[name])
            time.sleep(options.settle)
        name = Path(path).name
        result["fixtures"][name] = {
            "bytes": 0 if path == "none" else Path(path).stat().st_size,
            "runsWithoutPresentation": missing,
            **{key: quantiles(values) for key, values in samples.items() if values},
        }
        shown = result["fixtures"][name]
        print(
            f"{result['label']:<10} {name:<16}"
            + "".join(
                f"  {key} {shown[key]['median']:>8.1f}/{shown[key]['p95']:<8.1f}"
                for key in moments
                if key in shown
            )
        )

    if options.output:
        options.output.parent.mkdir(parents=True, exist_ok=True)
        options.output.write_text(json.dumps(result, indent=1) + "\n")


if __name__ == "__main__":
    main()
