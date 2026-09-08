//! Hashline — a fast, quiet Markdown viewer for Linux.
//!
//! The crate is a library with a thin binary on top so that the parts which do
//! not need a window — the block plan, the design tokens, the geometry — can be
//! tested without one (SPEC.md, section 12).

pub mod app;
pub mod layout;
pub mod theme;
pub mod view;
