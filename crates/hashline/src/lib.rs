//! Hashline — a fast, quiet Markdown viewer for Linux.
//!
//! The crate is a library with a thin binary on top so that the parts which do
//! not need a window — the block plan, the design tokens, the geometry — can be
//! tested without one (docs/development.md).

pub mod app;
pub mod document;
pub mod highlight;
pub mod layout;
pub mod outline;
pub mod preferences;
pub mod search;
pub mod theme;
pub mod view;
