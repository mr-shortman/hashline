//! The window's own stylesheet.
//!
//! The document is drawn from the token table in the module above, so it
//! follows the reader's choice of theme whatever the desktop is set to. The
//! widgets around it — header bar, search bar, outline, menu — took their
//! colours from the system GTK theme instead, and a system theme is free to be
//! dark in both of its variants: `Orchis-Dark` ships the same stylesheet as
//! `gtk.css` and as `gtk-dark.css`. The light document then sat in dark chrome.
//!
//! So the chrome is styled here, from the same tokens, at
//! `STYLE_PROVIDER_PRIORITY_APPLICATION`. That priority sits above the theme's,
//! which means every property named below wins over whatever the desktop
//! supplies, and every property not named below still comes from it — the
//! window keeps the system's window controls, its focus rings and its metrics.
//!
//! The selectors are the widget node names from
//! <https://docs.gtk.org/gtk4/css-overview.html>, not classes of our own, so a
//! widget picks the rules up without being told about them.
//!
//! The reasoning is in `docs/decisions/018-chrome-stylesheet.md`.

use std::cell::RefCell;

use super::{Color, Palette, Variant, MOTION_MS, RADIUS};

thread_local! {
    /// One provider for the display, reloaded when the theme changes. Adding a
    /// second provider per change would leave the first one in the cascade.
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

/// Applies the palette to the widgets around the document.
///
/// Called for the first theme resolution and for every change after it. The
/// provider is installed once; later calls only hand it new text.
pub fn apply(palette: Palette) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    PROVIDER.with(|cell| {
        let mut cell = cell.borrow_mut();
        let provider = cell.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            provider
        });
        provider.load_from_string(&stylesheet(palette));
    });
}

/// `#rrggbb`, or `rgba()` when the colour carries an alpha.
fn css(color: Color) -> String {
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    if color.alpha >= 1.0 {
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(color.red),
            channel(color.green),
            channel(color.blue)
        )
    } else {
        format!(
            "rgba({},{},{},{:.3})",
            channel(color.red),
            channel(color.green),
            channel(color.blue),
            color.alpha
        )
    }
}

/// The whole stylesheet for one palette.
///
/// It is built as text rather than kept as two files so that the two variants
/// cannot drift apart: there is one set of rules, and the tokens are
/// substituted into it.
fn stylesheet(palette: Palette) -> String {
    let bg = css(palette.bg);
    let surface = css(palette.surface);
    let subtle = css(palette.subtle);
    let text = css(palette.text);
    let muted = css(palette.muted);
    let border = css(palette.border);
    let accent = css(palette.accent);
    let accent_soft = css(palette.accent_soft);
    let error = css(palette.error);
    // The type on an accent fill. Neither token is a shade of the other, so
    // the readable partner is named rather than derived by lightening.
    let on_accent = match palette.variant {
        Variant::Light => css(palette.surface),
        Variant::Dark => css(palette.bg),
    };
    let shadow = match palette.variant {
        Variant::Light => "0 5px 20px rgba(0,0,0,0.10)",
        Variant::Dark => "0 5px 20px rgba(0,0,0,0.45)",
    };
    let radius = RADIUS;
    let motion = MOTION_MS;

    format!(
        "
/* The window and the areas that are simply the reading background. */
window.background {{
  background-color: {bg};
  color: {text};
}}
window.background > .titlebar:not(headerbar) {{
  background-color: {surface};
}}
label {{ color: inherit; }}
.dim-label, label.dim-label {{ color: {muted}; }}

/* Header bar. The window controls keep the system's own drawing. */
headerbar {{
  background-color: {surface};
  background-image: none;
  border-bottom: 1px solid {border};
  box-shadow: none;
  color: {text};
}}
headerbar:backdrop {{
  background-color: {surface};
  color: {muted};
}}
headerbar .title {{
  color: {text};
  font-weight: 600;
}}

/* Controls. The scale is the reference's: transparent until touched, the
   accent wash when pressed or held down. */
button {{
  background-color: transparent;
  background-image: none;
  border: 1px solid transparent;
  border-radius: {radius}px;
  box-shadow: none;
  color: {text};
  text-shadow: none;
  transition: background-color {motion}ms, border-color {motion}ms, color {motion}ms;
}}
button:hover {{ background-color: {subtle}; }}
button:active {{ background-color: {accent_soft}; color: {accent}; }}
button:checked {{ background-color: {accent_soft}; color: {accent}; }}
button:disabled {{ background-color: transparent; color: {muted}; }}
button.suggested-action {{
  background-color: {accent};
  background-image: none;
  color: {on_accent};
  border-color: transparent;
}}
button.suggested-action:hover {{ background-color: {accent}; }}
button image {{ color: inherit; }}
menubutton > button {{ color: {text}; }}

/* Text fields. */
entry, entry.search {{
  background-color: {bg};
  background-image: none;
  border: 1px solid {border};
  border-radius: {radius}px;
  box-shadow: none;
  color: {text};
  caret-color: {text};
}}
entry:focus-within {{
  border-color: {accent};
  box-shadow: none;
}}
entry > text > placeholder {{ color: {muted}; opacity: 1; }}
entry > image {{ color: {muted}; }}

/* The search bar over the document: a surface strip with one rule under it,
   as `.search-bar` in the design reference. */
searchbar > revealer > box {{
  background-color: {surface};
  background-image: none;
  border-bottom: 1px solid {border};
  box-shadow: none;
  padding: 8px 18px;
  color: {muted};
}}

/* The outline, as a sidebar and as an overlay. It is opaque in both: an
   overlay that lets the text through is unreadable. */
scrolledwindow.sidebar, .sidebar > viewport, .sidebar listview {{
  background-color: {surface};
  color: {text};
}}
scrolledwindow.sidebar {{ border-right: 1px solid {border}; }}
listview {{ background-color: transparent; }}
listview > row {{
  color: {muted};
  background-color: transparent;
  border-radius: 4px;
  transition: background-color {motion}ms, color {motion}ms;
}}
listview > row:hover {{ background-color: {subtle}; color: {text}; }}
listview > row:selected {{ background-color: {accent_soft}; color: {accent}; }}

/* Tabs, shown only from the second document on. */
notebook > header {{
  background-color: {subtle};
  border-color: {border};
  box-shadow: none;
}}
notebook > header > tabs > tab {{
  background-color: transparent;
  border-color: transparent;
  box-shadow: none;
  color: {muted};
}}
notebook > header > tabs > tab:checked {{
  background-color: {surface};
  color: {text};
}}

/* Popovers: the menu, and anything else GTK puts in one. */
popover > contents {{
  background-color: {surface};
  background-image: none;
  border: 1px solid {border};
  border-radius: 10px;
  box-shadow: {shadow};
  color: {text};
  padding: 8px;
}}
popover {{ background-color: transparent; }}
popover > arrow {{
  background-color: {surface};
  background-image: none;
  border: 1px solid {border};
}}
separator {{ background-color: {border}; }}

/* The menu's own rows and its two horizontal controls
   (`.menu-row`, `.theme-options`, `.zoom-options` in the reference). */
.menu-row {{ padding: 7px 10px; }}
.menu-shortcut {{ color: {muted}; }}
.settings-label {{ color: {muted}; }}
/* The theme switch is one control, not three buttons: a recessed track holds
   the segments, and the one in force is the only filled thing in it. The
   segments carry the menu's own type size rather than the button default, which
   is what made three words fill a panel-wide block. */
.theme-options {{
  background-color: {bg};
  border: 1px solid {border};
  border-radius: {radius}px;
  padding: 3px;
}}
.theme-options button {{
  border-radius: 5px;
  padding: 5px 8px;
  min-height: 0;
  font-size: 0.92em;
  font-weight: 500;
  color: {muted};
}}
.theme-options button:hover {{ background-color: {subtle}; color: {text}; }}
.theme-options button:checked {{
  background-color: {accent_soft};
  color: {accent};
  font-weight: 600;
}}
.zoom-options button {{ padding: 5px 9px; min-width: 26px; }}
.zoom-options label {{ font-size: 1.05em; }}

/* Messages. */
.error-text {{ color: {error}; }}

/* No fade at the top or the bottom of a scrolled area. GTK draws one whenever
   there is content out of view, so on a document of any length it appeared and
   disappeared under the search bar as the reader scrolled — motion the design
   does not have, over a document that is meant to sit still. */
scrolledwindow > undershoot.top,
scrolledwindow > undershoot.bottom,
scrolledwindow > undershoot.left,
scrolledwindow > undershoot.right,
scrolledwindow > overshoot.top,
scrolledwindow > overshoot.bottom,
scrolledwindow > overshoot.left,
scrolledwindow > overshoot.right,
scrolledwindow > junction {{
  background: none;
  background-image: none;
  background-color: transparent;
  box-shadow: none;
  border: none;
}}

/* Scrollbars, so they read against the document rather than the desktop. */
scrollbar {{ background-color: transparent; border: none; }}
scrollbar > range > trough {{ background-color: transparent; border: none; }}
scrollbar > range > trough > slider {{
  background-color: {muted};
  border: none;
}}
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DARK, LIGHT};

    #[test]
    fn every_token_reaches_the_stylesheet() {
        for palette in [LIGHT, DARK] {
            let sheet = stylesheet(palette);
            for token in [
                palette.bg,
                palette.surface,
                palette.subtle,
                palette.text,
                palette.muted,
                palette.border,
                palette.accent,
                palette.accent_soft,
                palette.error,
            ] {
                assert!(
                    sheet.contains(&css(token)),
                    "{} is missing from the {:?} stylesheet",
                    css(token),
                    palette.variant
                );
            }
        }
    }

    /// The light and the dark sheet differ only in the colours, never in which
    /// rules exist: one variant cannot grow a rule the other lacks.
    #[test]
    fn both_variants_have_the_same_rules() {
        let rule = |sheet: &str| {
            sheet
                .lines()
                .filter(|line| line.trim_end().ends_with('{'))
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(rule(&stylesheet(LIGHT)), rule(&stylesheet(DARK)));
    }

    #[test]
    fn colours_are_written_as_css() {
        assert_eq!(css(Color::hex(0x37684d)), "#37684d");
        assert_eq!(
            css(Color::hex(0x000000).with_alpha(0.5)),
            "rgba(0,0,0,0.500)"
        );
    }
}
