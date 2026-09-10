//! Watching the open file (SPEC.md, section 7).
//!
//! The **parent directory** is watched rather than the file, because that is
//! the only way a replacement survives: an editor that saves atomically writes
//! a temporary file and renames it over the target, which destroys the inode a
//! file watch is attached to. Events are then filtered down to the one name
//! that matters.

use std::path::{Path, PathBuf};

use notify::event::{AccessKind, AccessMode, ModifyKind};
use notify::{EventKind, RecursiveMode, Watcher as _};

/// Keeps a watch alive. Dropping it stops the watch, which is how a document
/// switch releases the previous one (SPEC.md, section 5).
pub struct Watch {
    _watcher: notify::RecommendedWatcher,
}

/// Watches `path` and calls `on_change` on the main thread whenever it may
/// have changed. Returns `None` if no watch could be established — manual
/// reload stays available in that case.
pub fn watch(path: &Path, on_change: impl Fn() + 'static) -> Option<Watch> {
    let directory = path.parent()?.to_path_buf();
    let name = path.file_name()?.to_os_string();
    let (sender, receiver) = async_channel::bounded(8);

    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        // Reading the file is not a change to it, and the reader reads it on
        // every reload: counting an open or a read as a reason to reload makes
        // the process wake itself up for as long as it runs
        // (crates/hashline/tests/reload.rs). Closing after *writing* is a save
        // and stays.
        if matches!(event.kind, EventKind::Access(kind)
            if !matches!(kind, AccessKind::Close(AccessMode::Write)))
        {
            return;
        }
        let named = event
            .paths
            .iter()
            .any(|changed| changed.file_name() == Some(name.as_os_str()));
        // A rename into place shows up as the temporary name too, so a create
        // or remove in the directory is worth a look as well.
        let touches_file =
            named || matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_));
        if !touches_file {
            return;
        }
        // Whether the file is whole again *already*. A rename into place, a
        // create, a remove and a close after writing each leave nothing half
        // written, so a quiet period after one of them is only waiting. A
        // write still in progress is the case the quiet period exists for, and
        // a create or remove under some other name is a hint, not a change.
        let settled = named
            && matches!(
                event.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(ModifyKind::Name(_))
                    | EventKind::Access(AccessKind::Close(AccessMode::Write))
            );
        let _ = sender.try_send(settled);
    })
    .ok()?;
    watcher
        .watch(&directory, RecursiveMode::NonRecursive)
        .ok()?;

    gtk::glib::spawn_future_local(async move {
        // Events arrive in bursts — one save is several of them, and a burst of
        // saves is many. Two different things keep that from becoming one
        // reload per event, and they sit on opposite sides of the reload.
        //
        // *Before* it: a write that is still running is waited out, because
        // reading a half written file renders half a document. The wait ends
        // as soon as the file is whole — the event that says so is enough, and
        // a quiet period is only the fallback for a writer that never sends
        // one. Waiting a fixed period after an atomic save spent 150 ms of the
        // 250 ms the reload budget allows on nothing at all, and waiting it
        // out in whole slices spent 280 ms, because the editor's temporary
        // file started a slice that the rename then arrived just after
        // (crates/hashline/tests/reload.rs).
        //
        // *After* it: a cooldown of the same length, so the rest of a burst
        // costs one more reload rather than one per save.
        //
        // The wait is bounded by `DEBOUNCE_CEILING_MS` from the first event, so
        // a file that is being written continuously still refreshes instead of
        // waiting for a quiet moment that never comes.
        let quiet = std::time::Duration::from_millis(DEBOUNCE_MS);
        let ceiling = std::time::Duration::from_millis(DEBOUNCE_CEILING_MS);
        loop {
            let Ok(mut settled) = receiver.recv().await else {
                return;
            };
            let first = std::time::Instant::now();
            while !settled && first.elapsed() < ceiling {
                // Cancelling this leaves the event in the channel, so a wait
                // that times out loses nothing.
                match gtk::glib::future_with_timeout(quiet, receiver.recv()).await {
                    Ok(Ok(next)) => settled = next,
                    Ok(Err(_)) => return,
                    Err(_) => break,
                }
            }
            while receiver.try_recv().is_ok() {}
            on_change();
            gtk::glib::timeout_future(quiet).await;
        }
    });

    Some(Watch { _watcher: watcher })
}

/// How long a write that is still running is waited out, and how long the
/// reload afterwards coalesces what follows (SPEC.md, section 7).
const DEBOUNCE_MS: u64 = 150;
/// How far the wait for an unfinished write may be extended by further events.
const DEBOUNCE_CEILING_MS: u64 = 750;

/// Where the reader is, in terms that survive the document being reparsed.
///
/// A heading id is the sturdiest handle available: it is derived from the text,
/// so it stays the same when paragraphs above it change, and the distance from
/// it to the top of the viewport keeps the same line at the same height. When
/// the heading is gone the block index is the fallback, and after that the top
/// of the document (SPEC.md, section 7).
#[derive(Clone, Debug, Default)]
pub struct Anchor {
    pub heading: Option<String>,
    pub block: usize,
    /// Distance from the anchor's top edge down to the top of the viewport.
    pub distance: f64,
}

/// A digest of the source, so that a watch event that changed nothing does not
/// cause a re-render (SPEC.md, section 7).
pub fn digest(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Resolves a path for display without leaking the whole home directory into
/// messages.
pub fn short_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// The directory relative paths in a document resolve against.
pub fn base_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::digest;

    #[test]
    fn identical_sources_have_identical_digests() {
        assert_eq!(digest("# Titel\n"), digest("# Titel\n"));
        assert_ne!(digest("# Titel\n"), digest("# Titel!\n"));
    }
}
