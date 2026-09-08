//! Watching the open file (SPEC.md, section 7).
//!
//! The **parent directory** is watched rather than the file, because that is
//! the only way a replacement survives: an editor that saves atomically writes
//! a temporary file and renames it over the target, which destroys the inode a
//! file watch is attached to. Events are then filtered down to the one name
//! that matters.

use std::path::{Path, PathBuf};

use notify::{RecursiveMode, Watcher as _};

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
        let touches_file = event.paths.iter().any(|changed| {
            changed.file_name() == Some(name.as_os_str())
                // A rename into place shows up as the temporary name too, so a
                // create or remove in the directory is worth a look as well.
                || matches!(
                    event.kind,
                    notify::EventKind::Create(_) | notify::EventKind::Remove(_)
                )
        });
        if touches_file {
            let _ = sender.try_send(());
        }
    })
    .ok()?;
    watcher
        .watch(&directory, RecursiveMode::NonRecursive)
        .ok()?;

    gtk::glib::spawn_future_local(async move {
        // Events arrive in bursts — a save is several of them. They are
        // coalesced by draining whatever is already queued and waiting out a
        // short quiet period before reacting (SPEC.md, section 7).
        while receiver.recv().await.is_ok() {
            gtk::glib::timeout_future(std::time::Duration::from_millis(DEBOUNCE_MS)).await;
            while receiver.try_recv().is_ok() {}
            on_change();
        }
    });

    Some(Watch { _watcher: watcher })
}

const DEBOUNCE_MS: u64 = 150;

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
