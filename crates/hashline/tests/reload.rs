//! The live-reload contract (docs/architecture.md).
//!
//! The watcher was written and never tested. What follows are the cases an
//! editor actually produces, and they are the reason the watch is on the
//! *directory* rather than on the file:
//!
//! * a save by rename, which is what an atomic writer does and which destroys
//!   the inode a file watch would be attached to,
//! * a delete followed by a fresh file under the same name,
//! * an intermediate state where the file on disk is half written,
//! * a burst of writes, which must cost one reload rather than one per write.
//!
//! Nothing here needs a display: the watch delivers into whatever main context
//! is the thread's default, and each test makes one of its own and drives it.
//! That is also what lets these run in parallel — a shared default context can
//! only be owned by one thread at a time.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use hashline::document::{digest, watch, Watch};

/// A main context of this test's own, made the thread's default while the
/// test runs. Everything the watch spawns runs in it, which is also what lets
/// these tests run in parallel.
struct Pump {
    context: gtk::glib::MainContext,
}

fn with_pump(body: impl FnOnce(&Pump)) {
    let context = gtk::glib::MainContext::new();
    let pump = Pump {
        context: context.clone(),
    };
    context
        .with_thread_default(|| body(&pump))
        .expect("a fresh context can be made the thread default");
}

impl Pump {
    /// Runs the context until `ready` or the deadline, and returns whether it
    /// became ready. Nothing here sleeps blindly: a fixed wait would either be
    /// flaky or slow, and this is neither.
    fn until(&self, ready: impl Fn() -> bool, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            while self.context.pending() {
                self.context.iteration(false);
            }
            if ready() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// A directory of this test's own. Two tests must never share one, because the
/// watch is on the directory and would see the other's writes.
struct Sandbox {
    path: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "hashline-reload-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("sandbox");
        Sandbox { path }
    }
    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Every reload the watch asked for, with the source that was on disk when it
/// asked. Reading the file inside the callback is exactly what the application
/// does, so a callback that fires too early is visible here as a short read.
#[derive(Default)]
struct Reloads {
    seen: RefCell<Vec<String>>,
}

impl Reloads {
    fn watching(path: &Path) -> (Rc<Self>, Watch) {
        let reloads = Rc::new(Reloads::default());
        let recorder = reloads.clone();
        let file = path.to_path_buf();
        let watch = watch(path, move || {
            let source = std::fs::read_to_string(&file).unwrap_or_default();
            recorder.seen.borrow_mut().push(source);
        })
        .expect("a watch on the sandbox");
        (reloads, watch)
    }
    fn count(&self) -> usize {
        self.seen.borrow().len()
    }
    fn last(&self) -> Option<String> {
        self.seen.borrow().last().cloned()
    }
}

/// How long a reload may take to be asked for: the 150 ms of coalescing the
/// watch does, plus room for the file system to report the change.
const REACTION: Duration = Duration::from_millis(2000);

/// Writes a whole file, the plain way an editor that truncates does.
fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write");
}

/// Writes a whole file the way an atomic editor does: a temporary beside it,
/// then a rename over the target. This replaces the inode.
fn save_by_rename(path: &Path, contents: &str) {
    let temporary = path.with_extension("md.tmp");
    std::fs::write(&temporary, contents).expect("write temporary");
    std::fs::rename(&temporary, path).expect("rename into place");
}

#[test]
fn a_save_by_rename_is_noticed_and_reports_the_new_text() {
    with_pump(|pump| {
        let sandbox = Sandbox::new("rename");
        let file = sandbox.file("doc.md");
        write(&file, "# Titel\n\nErster Text.\n");
        let (reloads, _watch) = Reloads::watching(&file);

        save_by_rename(&file, "# Titel\n\nZweiter Text.\n");
        assert!(
            pump.until(|| reloads.count() >= 1, REACTION),
            "a save by rename must be noticed"
        );
        assert_eq!(
            reloads.last().as_deref(),
            Some("# Titel\n\nZweiter Text.\n"),
            "the reload must see the renamed file, not the one it replaced"
        );
    });
}

#[test]
fn a_delete_and_a_fresh_file_under_the_same_name_are_noticed() {
    with_pump(|pump| {
        let sandbox = Sandbox::new("recreate");
        let file = sandbox.file("doc.md");
        write(&file, "# Titel\n\nErster Text.\n");
        let (reloads, _watch) = Reloads::watching(&file);

        std::fs::remove_file(&file).expect("remove");
        assert!(
            pump.until(|| reloads.count() >= 1, REACTION),
            "the file going away must be noticed"
        );
        // The file is gone, so the read fails and the application keeps what it
        // has. Nothing here may panic on that.
        assert_eq!(reloads.last().as_deref(), Some(""));

        let after_removal = reloads.count();
        write(&file, "# Titel\n\nWieder da.\n");
        assert!(
            pump.until(|| reloads.count() > after_removal, REACTION),
            "a file created again under the same name must be noticed"
        );
        assert_eq!(reloads.last().as_deref(), Some("# Titel\n\nWieder da.\n"));
    });
}

#[test]
fn a_half_written_file_settles_on_the_finished_text() {
    with_pump(|pump| {
        let sandbox = Sandbox::new("partial");
        let file = sandbox.file("doc.md");
        write(&file, "# Titel\n\nErster Text.\n");
        let (reloads, _watch) = Reloads::watching(&file);

        // An editor that truncates and writes in pieces: for a moment the file on
        // disk holds only the beginning of the new text.
        {
            let mut open = std::fs::File::create(&file).expect("truncate");
            open.write_all(b"# Titel\n\nHalb ges").expect("first half");
            open.flush().expect("flush");
            // Long enough for the watch's coalescing to expire on the half file.
            assert!(
                pump.until(|| reloads.count() >= 1, REACTION),
                "the truncation must be noticed"
            );
            open.write_all(b"chriebener Text.\n").expect("second half");
            open.flush().expect("flush");
        }
        let half = reloads.count();
        assert!(
            pump.until(|| reloads.count() > half, REACTION),
            "finishing the write must be noticed as well"
        );
        assert_eq!(
            reloads.last().as_deref(),
            Some("# Titel\n\nHalb geschriebener Text.\n"),
            "the last reload must hold the finished text"
        );
    });
}

#[test]
fn a_burst_of_writes_costs_one_reload_and_not_one_per_write() {
    with_pump(|pump| {
        let sandbox = Sandbox::new("burst");
        let file = sandbox.file("doc.md");
        write(&file, "# Titel\n\nStand 0.\n");
        let (reloads, _watch) = Reloads::watching(&file);

        // Twenty writes inside the coalescing window. Coalescing is the whole
        // reason the watch waits, and without it this is twenty re-renders.
        for step in 1..=20 {
            write(&file, &format!("# Titel\n\nStand {step}.\n"));
        }
        assert!(
            pump.until(|| reloads.count() >= 1, REACTION),
            "a burst must produce a reload"
        );
        // Let anything still queued arrive before counting.
        pump.until(|| false, Duration::from_millis(600));
        let count = reloads.count();
        assert!(
            count <= 3,
            "twenty writes in a burst became {count} reloads; coalescing is not working"
        );
        assert_eq!(
            reloads.last().as_deref(),
            Some("# Titel\n\nStand 20.\n"),
            "the reload must show the last write of the burst"
        );
    });
}

#[test]
fn a_write_that_changes_nothing_leaves_the_document_alone() {
    // The watch reports the touch; the digest is what keeps a re-render from
    // happening, which is the check the application makes before it reloads
    // (docs/architecture.md).
    with_pump(|pump| {
        let sandbox = Sandbox::new("identical");
        let file = sandbox.file("doc.md");
        let source = "# Titel\n\nUnveraendert.\n";
        write(&file, source);
        let (reloads, _watch) = Reloads::watching(&file);

        save_by_rename(&file, source);
        assert!(
            pump.until(|| reloads.count() >= 1, REACTION),
            "the touch itself is still noticed"
        );
        assert_eq!(
            digest(&reloads.last().unwrap()),
            digest(source),
            "an unchanged file must digest the same, so the reader keeps its document"
        );
    });
}

#[test]
fn reading_the_file_is_not_a_change_to_it() {
    // The reader reads the file on every reload, and the watch is on the
    // directory, so an open or a read reported as a change makes the process
    // wake itself up for as long as it runs. This is the test that found it.
    with_pump(|pump| {
        let sandbox = Sandbox::new("selfwake");
        let file = sandbox.file("doc.md");
        write(&file, "# Titel\n\nErster Text.\n");
        let (reloads, _watch) = Reloads::watching(&file);

        save_by_rename(&file, "# Titel\n\nZweiter Text.\n");
        assert!(pump.until(|| reloads.count() >= 1, REACTION));
        // The callback above has read the file once. Read it a few more times
        // for good measure, then let everything settle.
        for _ in 0..5 {
            let _ = std::fs::read_to_string(&file).expect("read");
        }
        let after = reloads.count();
        pump.until(|| false, Duration::from_millis(900));
        assert_eq!(
            reloads.count(),
            after,
            "reading the open file must not ask for a reload"
        );
    });
}

#[test]
fn a_reload_is_asked_for_within_the_time_the_budget_allows() {
    // The budget is p95 ≤ 250 ms from the save to *visible* text
    // (docs/metrics.md), and a rename
    // into place puts a whole file there in one step: there is nothing to wait
    // out, so what the watch spends here is what the parse, the layout and the
    // frame do not get.
    //
    // The temporary file is written while the main loop runs, because that is
    // what an editor saving into a watched directory looks like from inside a
    // running application. Seeing that creation used to restart the coalescing
    // period, and the rename then arrived just after a fresh one had begun:
    // 280 ms of the budget went on waiting, measured against 1 ms now.
    with_pump(|pump| {
        let sandbox = Sandbox::new("latency");
        let file = sandbox.file("doc.md");
        write(&file, "# Titel\n\nStand 0.\n");
        let (reloads, _watch) = Reloads::watching(&file);

        let mut samples = Vec::new();
        for step in 1..=20 {
            let temporary = file.with_extension("md.tmp");
            std::fs::write(&temporary, format!("# Titel\n\nStand {step}.\n"))
                .expect("write temporary");
            pump.until(|| false, Duration::from_millis(20));
            let before = reloads.count();
            let started = Instant::now();
            std::fs::rename(&temporary, &file).expect("rename into place");
            assert!(
                pump.until(|| reloads.count() > before, REACTION),
                "write {step} produced no reload"
            );
            samples.push(started.elapsed());
            // Out of the coalescing window, so the next write is its own event.
            pump.until(|| false, Duration::from_millis(250));
        }
        samples.sort();
        let rank = ((samples.len() as f64) * 0.95).ceil() as usize;
        let p95 = samples[rank.saturating_sub(1).min(samples.len() - 1)];
        assert!(
            p95 <= Duration::from_millis(50),
            "p95 from the rename to the reload request was {p95:?}; the whole budget to visible text is 250 ms"
        );
    });
}
