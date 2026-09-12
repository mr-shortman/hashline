//! Where the time between `exec` and the first frame with text goes.
//!
//! The protocol trace taken from outside the process sees the first
//! `wl_surface.attach` and nothing before it, so everything from the dynamic
//! loader to the first layout is one opaque number. That number was 240 ms
//! against a 120 ms budget while the parse cost 0,9 ms and the first screen
//! 17 ms, which says only that the time is not in the document path — not
//! where it is.
//!
//! These marks split it up, on the clock libwayland stamps its messages with
//! (`CLOCK_REALTIME`, truncated to 32 bits of microseconds, printed in
//! milliseconds), so a mark and a protocol message can be read on one
//! timeline. With `HASHLINE_BENCH_STAGES` set, each goes to stderr as it
//! happens, because the harness stops the process with a signal:
//!
//! ```text
//! HASHLINE_STAGE toolkit=146814.210
//! ```
//!
//! The six marks are the boundaries a startup actually has: entering `main`,
//! the toolkit being up, the read and parse of the file starting and
//! finishing, the document being in the view, and the first frame having been
//! drawn. Unset, a mark costs one read of a `OnceLock`.

/// Read once: an environment lookup per mark would itself be startup work.
fn reporting() -> bool {
    static REPORTING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REPORTING.get_or_init(|| std::env::var_os("HASHLINE_BENCH_STAGES").is_some())
}

/// Notes that `name` happened now.
pub fn mark(name: &str) {
    if !reporting() {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // The same truncation libwayland prints, so the two can be subtracted.
    let micros = (now.as_micros() as u64) % (1u64 << 32);
    eprintln!("HASHLINE_STAGE {name}={:.3}", micros as f64 / 1000.0);
}

/// Notes that `name` happened now, the first time it does.
///
/// Drawing happens sixty times a second for as long as the window is open; the
/// question this answers is when it happened once.
pub fn once(guard: &std::sync::Once, name: &str) {
    guard.call_once(|| mark(name));
}
