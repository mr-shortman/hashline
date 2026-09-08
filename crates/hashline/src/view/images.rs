//! Local pictures: which ones may be shown, how large, and cached as textures.
//!
//! Decoding is the largest remaining attack surface, because it hands foreign
//! binary data to a C decoder (SPEC.md, section 11). Two rules follow, and both
//! are enforced here rather than at the call site:
//!
//! * the budget is checked from the file's **header**, before anything is
//!   decoded, and
//! * a path is only read when it stays inside the document's own directory —
//!   after canonicalization, so a symlink cannot lead out of it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use gtk::gdk;
use gtk::gdk_pixbuf;

use crate::layout::ImageSource;

/// Pixels a decoded picture may occupy. Roughly a 40-megapixel photograph;
/// beyond that the placeholder says why.
const PIXEL_BUDGET: i64 = 40_000_000;

enum Entry {
    Ready {
        texture: gdk::Texture,
        width: f64,
        height: f64,
    },
    /// Refused, and it stays refused: retrying per frame would be a decode
    /// attempt per frame.
    Refused,
}

#[derive(Default)]
pub struct ImageCache {
    /// The directory of the open document; everything is resolved against it.
    base: RefCell<Option<PathBuf>>,
    entries: RefCell<HashMap<String, Entry>>,
}

impl ImageCache {
    pub fn set_base(&self, directory: Option<PathBuf>) {
        let changed = *self.base.borrow() != directory;
        if changed {
            *self.base.borrow_mut() = directory;
            self.entries.borrow_mut().clear();
        }
    }

    /// The texture for a source, if it was accepted.
    pub fn texture(&self, source: &str) -> Option<gdk::Texture> {
        self.load(source);
        match self.entries.borrow().get(source) {
            Some(Entry::Ready { texture, .. }) => Some(texture.clone()),
            _ => None,
        }
    }

    /// Resolves a document-relative source to a readable file inside the
    /// document's directory, or refuses it.
    fn resolve(&self, source: &str) -> Option<PathBuf> {
        // Remote pictures do not load in v1; the placeholder names the source
        // (SPEC.md, section 7).
        if source.contains("://") {
            return None;
        }
        let base = self.base.borrow().clone()?;
        let candidate = base.join(source);
        // Canonicalizing both sides is what makes an escaping symlink fail:
        // the check has to hold for the path that is actually read.
        let real = candidate.canonicalize().ok()?;
        let root = base.canonicalize().ok()?;
        real.starts_with(&root).then_some(real)
    }

    fn load(&self, source: &str) {
        if self.entries.borrow().contains_key(source) {
            return;
        }
        let entry = self.decode(source);
        self.entries.borrow_mut().insert(source.to_string(), entry);
    }

    fn decode(&self, source: &str) -> Entry {
        let Some(path) = self.resolve(source) else {
            return Entry::Refused;
        };
        // Header only: format and dimensions without decoding a pixel.
        let Some((_, width, height)) = gdk_pixbuf::Pixbuf::file_info(&path) else {
            return Entry::Refused;
        };
        if width <= 0 || height <= 0 || (width as i64) * (height as i64) > PIXEL_BUDGET {
            return Entry::Refused;
        }
        match gdk::Texture::from_filename(&path) {
            Ok(texture) => Entry::Ready {
                texture,
                width: width as f64,
                height: height as f64,
            },
            Err(_) => Entry::Refused,
        }
    }
}

impl ImageSource for ImageCache {
    fn intrinsic(&self, source: &str) -> Option<(f64, f64)> {
        self.load(source);
        match self.entries.borrow().get(source) {
            Some(Entry::Ready { width, height, .. }) => Some((*width, *height)),
            _ => None,
        }
    }
}
