//! Persisted preferences and reading positions (SPEC.md, sections 3 and 7).
//!
//! Everything here is optional. A missing or damaged schema must not stop the
//! program from opening a file — that is explicit in SPEC.md section 7 — so the
//! store degrades to holding nothing rather than failing.

use std::path::Path;

use gtk::gio;
use gtk::prelude::*;

use crate::document::Anchor;

pub const SCHEMA_ID: &str = "de.kalendium.Hashline";

/// At most this many paths are remembered (SPEC.md, section 7).
const MAX_POSITIONS: usize = 100;

#[derive(Clone)]
pub struct Preferences {
    schema: Option<gio::SettingsSchema>,
    settings: Option<gio::Settings>,
}

/// The schema, from the installed location or from the build tree.
fn find_schema() -> Option<gio::SettingsSchema> {
    if let Some(schema) =
        gio::SettingsSchemaSource::default().and_then(|source| source.lookup(SCHEMA_ID, true))
    {
        return Some(schema);
    }
    let directory = std::path::Path::new(env!("HASHLINE_DEV_SCHEMA_DIR"));
    gio::SettingsSchemaSource::from_directory(directory, None, true)
        .ok()?
        .lookup(SCHEMA_ID, true)
}

impl Preferences {
    /// Opens the store, or an inert one when no schema can be found.
    ///
    /// An installed Hashline finds its schema in the system directory. A build
    /// tree falls back to the copy `build.rs` compiled, so `cargo run` behaves
    /// like the installed program without anyone having to set
    /// `GSETTINGS_SCHEMA_DIR`.
    pub fn load() -> Self {
        Preferences {
            schema: find_schema(),
            settings: find_schema()
                .map(|schema| gio::Settings::new_full(&schema, gio::SettingsBackend::NONE, None)),
        }
    }

    pub fn is_available(&self) -> bool {
        self.settings.is_some()
    }

    /// A schema that exists but lacks a key is a *stale* schema, and reading
    /// that key would abort the process rather than return a default — GIO
    /// treats it as a programming error. A build tree hits this whenever the
    /// schema source gains a key, so every access is guarded and a stale
    /// schema degrades instead of killing the program (SPEC.md, section 7).
    fn get(&self, key: &str) -> Option<&gio::Settings> {
        let schema = self.schema.as_ref()?;
        schema.has_key(key).then_some(())?;
        self.settings.as_ref()
    }

    pub fn theme(&self) -> String {
        self.get("theme")
            .map(|settings| settings.string("theme").to_string())
            .unwrap_or_else(|| "system".into())
    }
    pub fn set_theme(&self, mode: &str) {
        if let Some(settings) = self.get("theme") {
            let _ = settings.set_string("theme", mode);
        }
    }

    pub fn zoom(&self) -> i32 {
        self.get("zoom")
            .map(|settings| settings.int("zoom"))
            .unwrap_or(100)
    }
    pub fn set_zoom(&self, percent: i32) {
        if let Some(settings) = self.get("zoom") {
            let _ = settings.set_int("zoom", percent);
        }
    }

    pub fn outline_visible(&self) -> bool {
        self.get("outline-visible")
            .map(|settings| settings.boolean("outline-visible"))
            .unwrap_or(false)
    }
    pub fn set_outline_visible(&self, visible: bool) {
        if let Some(settings) = self.get("outline-visible") {
            let _ = settings.set_boolean("outline-visible", visible);
        }
    }

    pub fn window_size(&self) -> (i32, i32, bool) {
        match self.get("window-width") {
            Some(settings) => (
                settings.int("window-width"),
                settings.int("window-height"),
                settings.boolean("window-maximized"),
            ),
            None => (900, 700, false),
        }
    }
    pub fn set_window_size(&self, width: i32, height: i32, maximized: bool) {
        if let Some(settings) = self.get("window-width") {
            // A maximized window's own size is not worth recording; the size
            // to restore is the one it had before.
            if !maximized {
                let _ = settings.set_int("window-width", width);
                let _ = settings.set_int("window-height", height);
            }
            let _ = settings.set_boolean("window-maximized", maximized);
        }
    }

    /// Where reading stopped in a file, if it is remembered.
    pub fn reading_position(&self, path: &Path) -> Option<Anchor> {
        let wanted = path.to_string_lossy().to_string();
        self.positions()
            .into_iter()
            .find(|(stored, _, _)| *stored == wanted)
            .map(|(_, heading, distance)| Anchor {
                heading: (!heading.is_empty()).then_some(heading),
                block: 0,
                distance: distance as f64,
            })
    }

    /// Records where reading stopped, newest first, bounded in length.
    pub fn remember_position(&self, path: &Path, anchor: &Anchor) {
        let Some(settings) = self.get("reading-positions") else {
            return;
        };
        let wanted = path.to_string_lossy().to_string();
        let mut positions = self.positions();
        positions.retain(|(stored, _, _)| *stored != wanted);
        positions.insert(
            0,
            (
                wanted,
                anchor.heading.clone().unwrap_or_default(),
                anchor.distance.max(0.0) as u64,
            ),
        );
        positions.truncate(MAX_POSITIONS);
        let value = positions
            .iter()
            .map(|(path, heading, distance)| (path.as_str(), heading.as_str(), *distance))
            .collect::<Vec<_>>()
            .to_variant();
        let _ = settings.set_value("reading-positions", &value);
    }

    fn positions(&self) -> Vec<(String, String, u64)> {
        self.get("reading-positions")
            .and_then(|settings| {
                settings
                    .value("reading-positions")
                    .get::<Vec<(String, String, u64)>>()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::Preferences;

    /// Round-trips through a real store, but only where one exists. Run with
    /// `GSETTINGS_SCHEMA_DIR=target/schemas GSETTINGS_BACKEND=memory` to
    /// exercise it; without the schema it reports that it was skipped rather
    /// than passing on nothing.
    #[test]
    fn a_reading_position_survives_a_round_trip() {
        let preferences = Preferences::load();
        if !preferences.is_available() {
            eprintln!("skipped: no installed schema for {}", super::SCHEMA_ID);
            return;
        }
        let path = std::path::Path::new("/docs/beispiel.md");
        let anchor = crate::document::Anchor {
            heading: Some("doc-kapitel".to_string()),
            block: 7,
            distance: 123.0,
        };
        preferences.remember_position(path, &anchor);
        let stored = preferences.reading_position(path).expect("a position");
        assert_eq!(stored.heading.as_deref(), Some("doc-kapitel"));
        assert_eq!(stored.distance, 123.0);
        // An unknown file has no position.
        assert!(preferences
            .reading_position(std::path::Path::new("/docs/anderes.md"))
            .is_none());

        for theme in ["dark", "light", "system"] {
            preferences.set_theme(theme);
            assert_eq!(preferences.theme(), theme);
        }
        preferences.set_zoom(140);
        assert_eq!(preferences.zoom(), 140);
        preferences.set_outline_visible(true);
        assert!(preferences.outline_visible());
    }

    #[test]
    fn a_missing_schema_yields_defaults_and_never_panics() {
        // The development tree has no installed schema unless one was
        // compiled into GSETTINGS_SCHEMA_DIR; either way this must hold.
        let preferences = Preferences::load();
        let zoom = preferences.zoom();
        assert!((80..=200).contains(&zoom), "zoom {zoom}");
        let (width, height, _) = preferences.window_size();
        assert!(width > 0 && height > 0);
        // Writing is a no-op without a schema and must not panic.
        preferences.set_outline_visible(true);
        let _ = preferences.outline_visible();
    }
}
