//! What the longest piece of main-thread work cost, for the budget that says
//! there must not be one over 16 ms (SPEC.md, section 9, and
//! docs/decisions/014-competitive-targets.md, section 3.3).
//!
//! A budget without an observer is a wish. This one cannot be measured from
//! outside the process: a compositor sees frames, not what happened between
//! them, and a frame that is late does not say which task was to blame. So the
//! reader times the work it does on the main thread itself and reports the
//! longest of it.
//!
//! Timed are the six places where the application does work on that thread:
//! laying out the blocks that came into view, filling the buffer around them,
//! loading one of the style's faces before a document needs it, drawing, being
//! given a new size, and taking on a parsed document. The parse and the file
//! read are not here because they are not on this thread. Reading the clock
//! twice costs tens of nanoseconds against work measured in milliseconds, so
//! it is not conditional; only the reporting is.
//!
//! Six places, not five, because every one of them has to be here for the
//! longest of them to mean anything: work moved out of a timed block into an
//! untimed one would lower the number without making a frame arrive sooner.
//!
//! With `HASHLINE_BENCH_MAIN_THREAD` set, the report goes to stderr as it
//! happens rather than at exit, because the measurement harness stops the
//! process with a signal:
//!
//! ```text
//! HASHLINE_BENCH mainThreadMaxMs=18.42 task=snapshot
//! ```
//!
//! Each line is a new maximum, so the last one is the answer.

use std::cell::Cell;
use std::time::Instant;

thread_local! {
    /// The longest task so far, in milliseconds. Main thread only, which is
    /// exactly the thread this measures.
    static LONGEST: Cell<f64> = const { Cell::new(0.0) };
}

/// Whether to report. Read once: an environment lookup per frame would itself
/// be main-thread work.
fn reporting() -> bool {
    static REPORTING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REPORTING.get_or_init(|| std::env::var_os("HASHLINE_BENCH_MAIN_THREAD").is_some())
}

/// Runs `work`, and reports it if it was the longest main-thread task so far.
pub fn timed<T>(task: &'static str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = work();
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    LONGEST.with(|longest| {
        if elapsed > longest.get() {
            longest.set(elapsed);
            if reporting() {
                eprintln!("HASHLINE_BENCH mainThreadMaxMs={elapsed:.2} task={task}");
            }
        }
    });
    result
}

/// The longest main-thread task so far, in milliseconds.
#[cfg(test)]
pub fn longest() -> f64 {
    LONGEST.with(|longest| longest.get())
}

/// Forgets it, so a check can measure one stretch of work rather than the
/// whole life of the process.
#[cfg(test)]
pub fn forget() {
    LONGEST.with(|longest| longest.set(0.0));
}
