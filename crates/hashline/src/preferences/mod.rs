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
    settings: Option<gio::Settings>,
}

impl Preferences {
    /// Opens the store, or an inert one when the schema is not installed —
    /// which is the normal state in a development tree until the schema is
    /// installed with the application in M3.
    pub fn load() -> Self {
        let installed = gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(SCHEMA_ID, true))
            .is_some();
        Preferences {
            settings: installed.then(|| gio::Settings::new(SCHEMA_ID)),
        }
    }

    pub fn is_available(&self) -> bool {
        self.settings.is_some()
    }

    pub fn zoom(&self) -> i32 {
        self.settings
            .as_ref()
            .map(|settings| settings.int("zoom"))
            .unwrap_or(100)
    }
    pub fn set_zoom(&self, percent: i32) {
        if let Some(settings) = &self.settings {
            let _ = settings.set_int("zoom", percent);
        }
    }

    pub fn outline_visible(&self) -> bool {
        self.settings
            .as_ref()
            .map(|settings| settings.boolean("outline-visible"))
            .unwrap_or(false)
    }
    pub fn set_outline_visible(&self, visible: bool) {
        if let Some(settings) = &self.settings {
            let _ = settings.set_boolean("outline-visible", visible);
        }
    }

    pub fn window_size(&self) -> (i32, i32, bool) {
        match &self.settings {
            Some(settings) => (
                settings.int("window-width"),
                settings.int("window-height"),
                settings.boolean("window-maximized"),
            ),
            None => (900, 700, false),
        }
    }
    pub fn set_window_size(&self, width: i32, height: i32, maximized: bool) {
        if let Some(settings) = &self.settings {
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
        let Some(settings) = &self.settings else {
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
        self.settings
            .as_ref()
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
