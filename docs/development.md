# Development

## Setup

```sh
sudo apt install libgtk-4-dev build-essential pkg-config zlib1g-dev
```

Rust is pinned to 1.98.1 in `rust-toolchain.toml`; rustup picks it up. No Node,
no frontend build. Extras: `python3-gi` and `gir1.2-atspi-2.0` for the desktop
tests, `sysprof` for frame timing.

## Build and run

```sh
cargo run -p hashline -- README.md
cargo build --release --workspace
```

`build.rs` compiles the GSettings schema on every build and the application
finds it there, so a development tree needs no `GSETTINGS_SCHEMA_DIR`.

| Variable | Effect |
| --- | --- |
| `GSK_RENDERER=cairo` | The renderer every number in [metrics.md](metrics.md) assumes |
| `HASHLINE_MONITOR=DP-3` | Open on that monitor; matched against connector, model, vendor, description |
| `HASHLINE_BENCH_STAGES=1` | Print six startup marks to stderr (`main`, `toolkit`, `parse`, `parsed`, `document`, `frame`) on the clock libwayland stamps with |
| `HASHLINE_BENCH_MAIN_THREAD=1` | Print every new longest main-thread task to stderr |

## Checks, the same ones CI runs

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
GSETTINGS_BACKEND=memory cargo test --workspace
```

`.github/workflows/check.yml` additionally regenerates and verifies the
fixtures, runs the benchmark suite's own unit tests, builds the `.deb` and
installs it in a fresh Ubuntu 26.04 container.

## Tests that need a window

These are not part of `cargo test --workspace`. Each needs **its own process**,
because GTK initializes once per process; hence the `--exact`. Both need a
compiled schema in a place of their own:

```sh
cargo build -p hashline
glib-compile-schemas --strict --targetdir=/tmp/hashline-test-schemas data
```

The UI integration test covers window reuse, tabs, menu actions, theme state,
escape order, selection, the text interface, reading anchors, links and
anchors. Run it against a headless GNOME Shell so nothing appears on your
screen:

```sh
dbus-run-session -- sh -c '
  gnome-shell --headless --wayland --no-x11 --virtual-monitor 1280x900 \
      --wayland-display=hashline-test >/dev/null 2>&1 & shell=$!
  until [ -S "$XDG_RUNTIME_DIR/hashline-test" ]; do sleep 0.1; done
  WAYLAND_DISPLAY=hashline-test GDK_BACKEND=wayland \
      GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory \
      cargo test -p hashline -- --ignored --exact app::tests::native_ui
  status=$?; kill $shell; exit $status'
```

The main-thread budget test scrolls the stress fixtures and reports the longest
task; it needs `python3 benchmarks/fixtures.py` first:

```sh
GSETTINGS_BACKEND=memory GSK_RENDERER=cairo cargo test --release -p hashline -- \
    --ignored --exact app::tests::main_thread_work_stays_inside_the_frame_budget --nocapture
```

AT-SPI (document role, Unicode text offsets, handover to a running instance):

```sh
dbus-run-session -- env GDK_BACKEND=x11 GTK_A11Y=atspi \
    GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory \
    python3 tests/desktop/native_reader.py target/debug/hashline
```

The live reload contract needs no window (rename-save, delete and recreate,
half-written files, bursts of writes, unchanged content):

```sh
GSETTINGS_BACKEND=memory cargo test --release -p hashline --test reload
```

## Tools without a window

```sh
# Lay out a document into a PNG with the widget's own layout code
cargo run --release -p hashline --example render -- README.md /tmp/out.png 900 [dark]

# Parse, block plan and first screen, measured separately;
# --geometry lays out every block once and compares estimated to measured height
cargo run --release -p hashline --example measure -- benchmarks/generated/small.md
```

## Benchmarks

[metrics.md](metrics.md) says what is measured and how. `benchmarks/README.md`
covers run management: resuming, time budgets, publishing results.

## Package

Target: Ubuntu 26.04 LTS, amd64. Build on the target distribution, because
`dpkg-shlibdeps` derives the runtime dependencies from the actual binary.

```sh
sudo apt install python3 dpkg-dev desktop-file-utils libglib2.0-bin shared-mime-info
python3 packaging/build_deb.py          # target/packages/hashline_<version>-1_amd64.deb
sudo apt install ./target/packages/hashline_*.deb
```

The package registers the desktop entry, MIME types, icon and schema, and never
changes an existing default handler. Isolated install test (install,
reconfigure, remove, reinstall, purge, desktop launch, instance handover,
AT-SPI, in a throwaway container):

```sh
docker build -f packaging/Dockerfile -t hashline-package-test .
docker run --rm hashline-package-test
```

Package builds are reproducible for the same source, toolchain, path and
`SOURCE_DATE_EPOCH`, which defaults to the last commit.

Removing keeps user settings:

```sh
sudo apt remove hashline
gsettings reset-recursively de.kalendium.Hashline   # only if you want them gone
```

The package registers Markdown support but never claims it. To make Hashline
the default by hand:

```sh
xdg-mime default de.kalendium.Hashline.desktop text/markdown
```
